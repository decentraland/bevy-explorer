//! The web entry: the headless app run as a second instance of this module over its own
//! memory, beside the client's. A separate memory gives it its own statics, so the process-wide
//! server flag (`common::structs::set_server_mode`) holds for this instance only. It has no
//! render worker, no winit and no asset loader worker (assets load on the engine worker).

use std::{cell::RefCell, rc::Rc, str::FromStr, time::Duration};

use bevy::platform::collections::HashMap;
use bevy::{
    app::{PluginsState, TaskPoolThreadAssignmentPolicy},
    log::{Level, LogPlugin},
    prelude::*,
    web_worker::WorkerSpec,
};
use common::structs::IVec2Arg;
use dcl::SceneLogMessage;
use dcl_wasm::init_runtime;
use system_api_types::launch_options::LaunchOptions;
use wallet::Wallet;
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_futures::js_sys;
use web_time::Instant;

const TICK_HZ: u32 = 30;
const SCENE_THREADS: usize = 1;
const COMPUTE_THREADS: u32 = 0;

/// Page side: starts the server's workers. `options` is an `engine_run` options object (the
/// launch options are read, the client's ignored), `glue_url` the url of this module's JS glue
/// for the workers to import. Returns `{ engine, compute: [...] }`, the workers.
#[wasm_bindgen]
pub fn headless_start(options: JsValue, glue_url: String) -> Result<JsValue, JsValue> {
    launch_options(&options)?;
    let compute = (0..COMPUTE_THREADS)
        .map(|i| {
            bevy::web_worker::spawn_worker(
                &glue_url,
                &WorkerSpec {
                    entry: "__bevy_compute_worker_main".into(),
                    args: Vec::new(),
                    tag: format!("server compute-{i}"),
                    stack_size: Some(4 * 1024 * 1024),
                    transfer: Vec::new(),
                },
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let engine = bevy::web_worker::spawn_worker(
        &glue_url,
        &WorkerSpec {
            entry: "headless_run".into(),
            args: vec![options],
            tag: "server engine".into(),
            stack_size: Some(10 * 1024 * 1024),
            transfer: Vec::new(),
        },
    )?;
    let result = js_sys::Object::new();
    js_sys::Reflect::set(&result, &"engine".into(), &engine)?;
    js_sys::Reflect::set(
        &result,
        &"compute".into(),
        &compute.iter().collect::<js_sys::Array>(),
    )?;
    Ok(result.into())
}

/// A server in the page is a local-dev flow, as the native binary's standalone `--server-mode`
/// is: it serves a preview realm only.
fn launch_options(options: &JsValue) -> Result<LaunchOptions, JsValue> {
    let launch = crate::web::parse_options(options)?.launch;
    if !launch.preview {
        return Err(JsValue::from_str(
            "headless: a scene server in the page requires `preview`",
        ));
    }
    Ok(launch)
}

/// Engine worker entry. Throws (rejects the launch) on an invalid option.
#[wasm_bindgen]
pub fn headless_run(options: JsValue) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();

    let launch = launch_options(&options)?;
    crate::launch::latch(&launch).map_err(|e| JsValue::from_str(&format!("headless_run: {e}")))?;
    let realm = launch
        .realm
        .clone()
        .unwrap_or_else(|| "http://localhost:8000".to_owned());
    let location = match &launch.position {
        Some(position) => IVec2Arg::from_str(position).map_err(|e| e.to_string())?.0,
        None => IVec2::ZERO,
    };
    common::structs::set_server_mode();
    init_runtime();

    let config = super::config(&realm, location, TICK_HZ, SCENE_THREADS, true);

    let mut app = App::new();
    app.add_plugins(LogPlugin {
        level: Level::INFO,
        filter: "symphonia=warn".to_owned(),
        custom_layer: |_| None,
    });
    // as the client (lib.rs wasm_default_plugins): only the compute pool has real threads
    let fixed = |n: usize| TaskPoolThreadAssignmentPolicy {
        min_threads: n,
        max_threads: n,
        percent: 1.0,
        on_thread_spawn: None,
        on_thread_destroy: None,
    };
    let compute_threads = COMPUTE_THREADS as usize;
    app.add_plugins(TaskPoolPlugin {
        task_pool_options: TaskPoolOptions {
            min_total_threads: compute_threads + 2,
            max_total_threads: compute_threads + 2,
            io: fixed(1),
            async_compute: fixed(1),
            compute: fixed(compute_threads),
        },
    });
    app.set_runner(run_on_worker_timer);

    let mut wallet = Wallet::default();
    wallet.finalize_as_guest();

    super::assemble(&mut app, config, &launch, &realm, true, wallet);
    app.add_systems(PreUpdate, (report, forward_scene_logs));

    app.run();
    Ok(())
}

/// `ScheduleRunnerPlugin`'s loop for a worker: bevy's reads `window`, which a worker doesn't
/// have. Returns once the first tick is scheduled. The timer paces the ticks: the scene loop's
/// own frame sleep is native-only (the web client is paced by its display).
fn run_on_worker_timer(mut app: App) -> AppExit {
    if app.plugins_state() != PluginsState::Cleaned {
        app.finish();
        app.cleanup();
    }
    let interval = Duration::from_secs(1) / TICK_HZ;
    let global: web_sys::DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();
    let schedule = move |tick: &Closure<dyn FnMut()>, delay: Duration| {
        global
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                tick.as_ref().unchecked_ref(),
                delay.as_millis() as i32,
            )
            .expect("setTimeout");
    };
    let tick: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::default();
    let next = tick.clone();
    let reschedule = schedule.clone();
    // Each tick is due an interval after the last was DUE, not after it ran, so a timer that
    // fires late (they do, by a millisecond or several) is repaid by a shorter wait and the
    // rate holds; as the scene loop's `next_frame_end` does for its native sleep.
    let mut due = Instant::now();
    *tick.borrow_mut() = Some(Closure::new(move || {
        app.update();
        if app.should_exit().is_none() {
            let now = Instant::now();
            due = (due + interval).max(now);
            reschedule(next.borrow().as_ref().unwrap(), due - now);
        }
    }));
    schedule(tick.borrow().as_ref().unwrap(), Duration::ZERO);
    AppExit::Success
}

/// Liveness lines, as the native binary's supervisor prints.
/// Each scene's console output, posted to the page (headless.js) for the editor to show, as the
/// native server's `@scene-log` lines are.
fn forward_scene_logs(
    scenes: Query<(
        Entity,
        &scene_runner::renderer_context::RendererSceneContext,
    )>,
    mut receivers: Local<HashMap<Entity, common::util::RingBufferReceiver<SceneLogMessage>>>,
) {
    let Ok(scope) = js_sys::global().dyn_into::<web_sys::DedicatedWorkerGlobalScope>() else {
        return;
    };
    for (ent, ctx) in scenes.iter() {
        let rx = receivers.entry(ent).or_insert_with(|| {
            let (_missed, backlog, rx) = ctx.logs.read();
            for log in backlog {
                post_scene_log(&scope, &log);
            }
            rx
        });
        while let Ok(log) = rx.try_recv() {
            post_scene_log(&scope, &log);
        }
    }
    receivers.retain(|ent, _| scenes.contains(*ent));
}

fn post_scene_log(scope: &web_sys::DedicatedWorkerGlobalScope, log: &SceneLogMessage) {
    let level = match log.level {
        dcl::SceneLogLevel::Log => "log",
        dcl::SceneLogLevel::SceneError => "error",
        dcl::SceneLogLevel::SystemError => "system",
    };
    let line = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&line, &"level".into(), &level.into());
    let _ = js_sys::Reflect::set(&line, &"msg".into(), &log.message.as_str().into());
    let message = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&message, &"sceneLog".into(), &line);
    let _ = scope.post_message(&message);
}

fn report(
    time: Res<Time>,
    scenes: Query<&scene_runner::renderer_context::RendererSceneContext>,
    mut announced: Local<bool>,
    mut last_report: Local<f64>,
) {
    let elapsed = time.elapsed_secs_f64();
    let count = scenes.iter().count();
    let max_tick = scenes.iter().map(|ctx| ctx.tick_number).max().unwrap_or(0);
    if count > 0 && max_tick >= 1 && !*announced {
        *announced = true;
        info!("[headless] {count} scene(s) live, first tick reached");
    }
    if elapsed - *last_report > 5.0 {
        *last_report = elapsed;
        info!("[headless] alive: {count} scene(s), max_tick={max_tick}, t={elapsed:.0}s");
    }
}
