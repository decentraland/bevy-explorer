use bevy::{
    asset::WasmLoaderHandle,
    log::{Level, LogPlugin},
    prelude::*,
    render::{render_resource::PipelineCompilationHandler, renderer::RenderDevice},
    tasks::BoxedFuture,
    web_worker::{WebWorkerConfig, WorkerSpec},
    winit::{UpdateMode, WinitSettings},
};
use bevy_console::ConsoleConfiguration;
use common::{
    rpc::RpcResultSender,
    structs::{AppConfig, CurrentRealm, EditorMode, PreviewMode, PrimaryUser, StartupScenes},
};
use dcl_wasm::init_runtime;
use futures_lite::io::AsyncReadExt;
use input_manager::InputPriorities;
use once_cell::sync::OnceCell;
use scene_runner::vec3_to_parcel;
use system_bridge::{SystemApi, SystemBridge};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::js_sys;
use web_sys::{HtmlCanvasElement, OffscreenCanvas};

use system_api_types::launch_options::{ClientOptions, EngineRunOptions, LaunchOptions};

use crate::{DecentralandApp, DecentralandAppConfig, DecentralandArguments};

static WASM_ASSET_LOADER_HANDLE: OnceCell<WasmLoaderHandle> = OnceCell::new();
static INIT_DATA: OnceCell<AppConfig> = OnceCell::new();
static CONSOLE_BRIDGE_SENDER: OnceCell<tokio::sync::mpsc::UnboundedSender<SystemApi>> =
    OnceCell::new();
/// The options the page launched with; the url sync echoes them back with the live values
/// (realm, position, …) swapped in.
static LAUNCH_OPTIONS: OnceCell<EngineRunOptions> = OnceCell::new();

// The page/engine-worker/render-worker split: the page owns the canvas and winit's DOM side,
// the engine worker runs the app world and winit's event loop, and the render worker owns the
// OffscreenCanvas, the wgpu device and the render world. `bevy::web_worker` spawns them and
// carries the startup handoffs; the entries below are the exports its workers call.

// Page functions (boot.js / engine.js define them on the window): looked up on `self`, which
// on the engine worker is the relay bevy::web_worker installs for them.
#[bevy::web_worker::page_functions]
#[wasm_bindgen(js_namespace = self)]
extern "C" {
    #[wasm_bindgen(js_name = _buildEngineApi)]
    fn build_engine_api(json: &str);

    /// The engine's current launch options as JSON — the `engine_run` options object minus the
    /// page-derived keys — for boot.js to mirror into the page url.
    #[wasm_bindgen(js_name = set_url_params)]
    fn set_url_params(options_json: &str);

    /// Ping the JS-side watchdog once per frame. If these stop arriving (e.g. the
    /// main thread is deadlocked waiting on a lock held by a crashed worker), the
    /// watchdog surfaces the crash overlay. Defined in index.html before the engine runs.
    #[wasm_bindgen(js_name = "__engineHeartbeat")]
    fn engine_heartbeat();

    /// Mirror "an engine text field holds keyboard focus" to the page (boot.js stores it
    /// on window.__engineTextFocus). The react HUD's hotkey handlers see key events before
    /// the engine does (capture-phase window listener vs the canvas), and an engine-rendered
    /// text field is invisible to their DOM-focus checks — this flag is how they know to
    /// leave keys alone while the user types into scene UI.
    #[wasm_bindgen(js_name = "__setEngineTextFocus")]
    fn set_engine_text_focus(focused: bool);

    /// Shows or hides the page's `#shader-compiling` indicator (the HUD's `renderBusy` probe)
    /// while gpu_cache.js compiles pipelines asynchronously on the render worker; boot.js
    /// defines it, gpu_cache.js calls the relay directly.
    #[allow(dead_code)]
    #[wasm_bindgen(js_name = "__setShaderCompiling")]
    fn set_shader_compiling(on: bool);

    /// The render worker's gpu cache is warm (`engine_render_setup` done): boot.js completes
    /// the loading bar's gpu step.
    #[wasm_bindgen(js_name = "__gpuCacheReady")]
    fn gpu_cache_ready();

    /// The page's `navigator.clipboard` (a worker's navigator has none), for copypwasmta: see
    /// `engine_run`.
    #[wasm_bindgen(js_name = "__copyToClipboard")]
    fn page_copy_to_clipboard(text: &str) -> js_sys::Promise;
    #[wasm_bindgen(js_name = "__readClipboard")]
    fn page_read_clipboard() -> js_sys::Promise;
}

fn read_clipboard() -> copypwasmta::wasm_clipboard::ClipboardFuture<String> {
    Box::pin(async {
        let text = wasm_bindgen_futures::JsFuture::from(page_read_clipboard())
            .await
            .map_err(|e| format!("{e:?}"))?;
        Ok(text.as_string().unwrap_or_default())
    })
}

fn write_clipboard(text: String) -> copypwasmta::wasm_clipboard::ClipboardFuture<()> {
    Box::pin(async move {
        wasm_bindgen_futures::JsFuture::from(page_copy_to_clipboard(&text))
            .await
            .map(|_| ())
            .map_err(|e| format!("{e:?}"))
    })
}

// gpu_cache.js: the device-level cache and async pipeline creation, run on the render worker
// where the device lives.
#[wasm_bindgen(module = "/deploy/web/engine/gpu_cache.js")]
extern "C" {
    #[wasm_bindgen(js_name = initGpuCache)]
    fn init_gpu_cache(key: &str) -> js_sys::Promise;
}

// The pipeline-compilation hooks `PipelineHandler` calls, which gpu_cache.js defines on the
// global of every realm that loads the glue.
#[wasm_bindgen(js_namespace = self)]
extern "C" {
    #[wasm_bindgen(js_name = "allowADummyPipeline")]
    fn allow_a_dummy_pipeline();

    #[wasm_bindgen(js_name = "lastPipelineWasValid")]
    fn last_pipeline_was_valid() -> bool;

    #[wasm_bindgen(js_name = "waitForPipelines")]
    fn wait_for_async_pipelines() -> js_sys::Promise;
}

/// Page side: forwards the browser's pointer-lock state to the engine (see platform/wasm.rs).
#[wasm_bindgen]
pub fn report_pointer_lock(locked: bool) {
    platform::report_pointer_lock(locked);
}

// The page/worker entry points of the media hosts. Defined here rather than in their crates:
// a `#[wasm_bindgen]` export in a dependency is only linked in if the root crate references it.

/// Page: starts the html media host (scene video/audio elements), transferring video frames to
/// `render_worker`. Before the engine worker starts.
#[wasm_bindgen]
pub fn media_host_main(render_worker: web_sys::Worker) {
    media::html::host::media_host_main(render_worker);
}

/// Page: starts the WebAudio host for scene audio sources. Before the engine worker starts.
#[wasm_bindgen]
pub fn audio_host_main() {
    av::audio_host_wasm::audio_host_main();
}

/// Page: starts the livekit host (the livekit-client rooms, tracks and mic live on the page).
/// Before the engine worker starts.
#[cfg(feature = "livekit")]
#[wasm_bindgen]
pub fn livekit_host_main() {
    comms::livekit::web::host::livekit_host_main();
}

/// call from a separate worker to initialize a channel for asset load processing
#[wasm_bindgen]
pub fn init_asset_load_thread() {
    let asset_server_channel = bevy::asset::init_thread_loader();
    let Ok(()) = WASM_ASSET_LOADER_HANDLE.set(asset_server_channel) else {
        panic!("can't init wasm loader");
    };
}

/// Asset processor worker entry: the image processor's channels, then its loop. The loop parks
/// this worker on the shared-memory request channel, so it starts from a later task: the entry
/// has to return first for the page to see the worker ready.
#[wasm_bindgen]
pub fn image_processor_main() -> Result<(), JsValue> {
    image_processing::image_processor_init();
    let global: web_sys::DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();
    let run = Closure::once_into_js(|| {
        wasm_bindgen_futures::spawn_local(image_processing::image_processor_run());
    });
    global.set_timeout_with_callback_and_timeout_and_arguments_0(run.unchecked_ref(), 0)?;
    Ok(())
}

#[wasm_bindgen]
pub async fn engine_init() -> Result<JsValue, JsValue> {
    console_error_panic_hook::set_once();

    let mut file = match web_fs::File::open("config.json").await {
        Ok(f) => f,
        Err(e) => {
            warn!("no config found: {e:?}");
            return Ok("No Config".into());
        }
    };
    let mut buf = String::new();
    if let Err(e) = file.read_to_string(&mut buf).await {
        warn!("failed to read config.json: {e:?}");
        return Ok("failed to read".into());
    }

    let Ok(mut config) = serde_json::from_str::<AppConfig>(&buf) else {
        warn!("failed to deserialize app config, using default");
        return Ok("failed to deserialize".into());
    };
    config.reset_outdated_settings();

    let _ = INIT_DATA.set(config);

    Ok("Config loaded".into())
}

/// The persisted home scene — the pinned realm (null = none pinned: the HUD substitutes its own
/// default realm, since this runs before `engine_run` latches the base domain the engine would
/// compose one from) + "x,y" parcel — as a JSON string. Valid after [`engine_init`] (it reads the
/// loaded config); exposed so the HUD's places picker can target home from "Skip" BEFORE the
/// engine is launched.
#[wasm_bindgen]
pub fn engine_home_scene() -> String {
    let (realm, parcel) = INIT_DATA
        .get()
        .map(|config| (config.home_realm.clone(), config.home_location()))
        .unwrap_or_else(|| (None, AppConfig::default().home_location()));
    serde_json::json!({ "realm": realm, "parcel": format!("{},{}", parcel.x, parcel.y) })
        .to_string()
}

/// Round-trip the page's object through JSON rather than `serde_wasm_bindgen::from_value`: that
/// only visits the struct's own fields, so `deny_unknown_fields` would never see a misspelt key.
fn parse_options(options: &JsValue) -> Result<EngineRunOptions, JsValue> {
    let json = String::from(js_sys::JSON::stringify(options)?);
    EngineRunOptions::from_json(&json)
        .map(EngineRunOptions::without_empty_strings)
        .map_err(|e| JsValue::from_str(&format!("engine_run: invalid options: {e}")))
}

fn web_worker_config(glue_url: String, options: JsValue, compute_threads: u32) -> WebWorkerConfig {
    WebWorkerConfig {
        glue_url,
        engine_entry: "engine_run".into(),
        engine_args: options,
        render_setup: Some("engine_render_setup".into()),
        compute_threads,
        // The engine worker gets the main-thread stack size (.cargo/config.toml -z stack-size);
        // the others need less.
        render_stack_size: Some(4 * 1024 * 1024),
        compute_stack_size: Some(4 * 1024 * 1024),
        engine_stack_size: Some(10 * 1024 * 1024),
    }
}

thread_local! {
    /// The render worker `engine_prepare_render` spawned ahead of `engine_start`.
    static RENDER_WORKER: std::cell::RefCell<Option<bevy::web_worker::RenderWorker>> =
        const { std::cell::RefCell::new(None) };
}

/// Page side: spawns the render worker on `canvas` ahead of `engine_start`, so its setup
/// (`engine_render_setup`: the gpu cache's precache) runs while the user is still choosing
/// where to go; `__gpuCacheReady` reports when it is done. `engine_start` attaches the rest.
#[wasm_bindgen]
pub fn engine_prepare_render(canvas: HtmlCanvasElement, glue_url: String) -> Result<(), JsValue> {
    let render =
        bevy::web_worker::start_render(canvas, &web_worker_config(glue_url, JsValue::NULL, 0))?;
    RENDER_WORKER.with(|slot| *slot.borrow_mut() = Some(render));
    Ok(())
}

/// Page side: starts the engine on `canvas`. `options` is the `engine_run` options object,
/// `glue_url` the url of this module's JS glue for the workers to import. Returns
/// `{ engine, render, compute: [...] }`, the workers.
#[wasm_bindgen]
pub fn engine_start(
    canvas: HtmlCanvasElement,
    options: JsValue,
    glue_url: String,
) -> Result<JsValue, JsValue> {
    let compute_threads = parse_options(&options)?
        .client
        .compute_threads
        .map_or_else(default_compute_threads, |n| n as u32);
    let config = web_worker_config(glue_url, options, compute_threads);
    let workers = match RENDER_WORKER.with(|slot| slot.borrow_mut().take()) {
        Some(render) => bevy::web_worker::start_with_render(render, &config)?,
        None => bevy::web_worker::start(canvas, &config)?,
    };
    let result = js_sys::Object::new();
    js_sys::Reflect::set(&result, &"engine".into(), &workers.engine)?;
    js_sys::Reflect::set(&result, &"render".into(), &workers.render)?;
    js_sys::Reflect::set(
        &result,
        &"compute".into(),
        &workers.compute.iter().collect::<js_sys::Array>(),
    )?;
    Ok(result.into())
}

/// Compute workers when `computeThreads` is absent: the cores left after the page, the
/// engine, render and two asset workers, at most 4 and at least 1.
fn default_compute_threads() -> u32 {
    let cores = web_sys::window()
        .map(|window| window.navigator().hardware_concurrency())
        .filter(|cores| *cores >= 1.0)
        .unwrap_or(4.0) as u32;
    cores.saturating_sub(5).clamp(1, 4)
}

/// Page side: spawns a further worker of ours that calls the export `entry` once its wasm
/// instance is up (the asset loader and processor). Posts `{ __bevy_ready: tag }` when it has.
#[wasm_bindgen]
pub fn engine_spawn_worker(
    glue_url: String,
    entry: String,
    tag: String,
) -> Result<web_sys::Worker, JsValue> {
    bevy::web_worker::spawn_worker(
        &glue_url,
        &WorkerSpec {
            entry,
            args: Vec::new(),
            tag,
            stack_size: None,
            transfer: Vec::new(),
        },
    )
}

/// Render worker setup, before bevy creates the render world there: gpu_cache.js patches the
/// worker's WebGPU device creation with its device-level cache and precreates the last run's
/// pipelines, as it did on the page when the device lived there.
#[wasm_bindgen]
pub async fn engine_render_setup(_canvas: OffscreenCanvas) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    wasm_bindgen_futures::JsFuture::from(init_gpu_cache(&assets::gpu_cache_hash())).await?;
    gpu_cache_ready();
    Ok(())
}

/// Engine worker entry (`bevy::web_worker` calls it once the worker is attached). `options` is
/// the `engine_run` object keyed by the web param table (one key per launch option; absent =
/// the engine's default). Throws (rejects the launch) on an invalid one.
#[wasm_bindgen]
pub fn engine_run(options: JsValue) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    let compute_threads = bevy::web_worker::compute_threads();
    copypwasmta::set_external_clipboard(copypwasmta::ExternalClipboard {
        get: read_clipboard,
        set: write_clipboard,
    });

    let options = parse_options(&options)?;
    let _ = LAUNCH_OPTIONS.set(options.clone());
    // the shared launch options' globals (src/launch.rs) — before anything composes a backend url
    crate::launch::latch(&options.launch)
        .map_err(|e| JsValue::from_str(&format!("engine_run: {e}")))?;
    init_runtime();

    let default_filter = "symphonia=warn";
    let filter = match std::option_env!("RUST_LOG") {
        Some(env) if !env.is_empty() => format!("{default_filter},{env}"),
        _ => default_filter.to_string(),
    };
    let decentraland_app = DecentralandApp::new(LogPlugin {
        level: Level::INFO,
        filter,
        custom_layer: |_| None,
    });

    let decentraland_app_config = DecentralandAppConfig::new(
        decentraland_serialized_app_config(),
        decentraland_app_arguments(&options),
        Some(WASM_ASSET_LOADER_HANDLE.get().unwrap().clone()),
        compute_threads as usize,
    );

    let mut app = decentraland_app.build(decentraland_app_config);

    // on wasm we need to explicitly specify key binds for the platform
    let user_agent = js_sys::global()
        .dyn_into::<web_sys::WorkerGlobalScope>()
        .ok()
        .and_then(|scope| scope.navigator().user_agent().ok())
        .unwrap_or_default();
    let text_bindings = if user_agent.contains("Mac") {
        bevy_simple_text_input::TextInputNavigationBindings::macos_default()
    } else {
        bevy_simple_text_input::TextInputNavigationBindings::non_macos_default()
    };
    app.insert_resource(text_bindings);

    // The systems that call the page hooks take `NonSend<JsThread>` so the multi-threaded
    // executor keeps them on this thread (the hooks are shims on this worker's global).
    app.insert_non_send_resource(JsThread);
    app.add_systems(Update, update_winit_fps)
        .add_systems(Update, update_url_params)
        .add_systems(Update, update_text_focus)
        .add_systems(Last, engine_heartbeat_system);

    app.add_systems(
        Update,
        extract_js_api.run_if(|mut once: Local<bool>| {
            let run = !*once;
            *once = true;
            run
        }),
    );

    let bridge_sender = app.world().resource::<SystemBridge>().sender.clone();
    let _ = CONSOLE_BRIDGE_SENDER.set(bridge_sender);

    app.run();
    Ok(())
}

/// Send a console command to the engine from JavaScript.
/// `command_line` is the full command string, e.g. `"/teleport 10 20"`.
/// Returns a Promise that resolves with the command output or rejects with an error message.
#[wasm_bindgen]
pub async fn engine_console_command(command_line: String) -> Result<JsValue, JsValue> {
    let mut parts = command_line.split_whitespace();
    let Some(cmd) = parts.next() else {
        return Err(JsValue::from_str("empty command"));
    };
    let cmd = if cmd.starts_with('/') {
        cmd.to_string()
    } else {
        format!("/{cmd}")
    };
    let args: Vec<String> = parts.map(String::from).collect();

    let Some(sender) = CONSOLE_BRIDGE_SENDER.get() else {
        return Err(JsValue::from_str("engine not initialized"));
    };

    let (sx, rx) = RpcResultSender::channel();
    sender
        .send(SystemApi::ConsoleCommand(cmd, args, sx))
        .map_err(|_| JsValue::from_str("engine channel closed"))?;

    rx.await
        .map_err(|_| JsValue::from_str("command response dropped"))?
        .map(|s| JsValue::from_str(&s))
        .map_err(|e| JsValue::from_str(&e))
}

/// Marker resource: a system taking `NonSend<JsThread>` runs on the engine worker, the thread
/// whose JS global has the page hooks.
struct JsThread;

/// Extract console command metadata from clap and store as JSON for the JS API.
fn extract_js_api(config: Res<ConsoleConfiguration>, _js: NonSend<JsThread>) {
    let commands: Vec<serde_json::Value> = config
        .commands
        .iter()
        .map(|(name, cmd)| {
            let trailing = cmd.is_trailing_var_arg_set();
            let positional: Vec<_> = cmd
                .get_arguments()
                .filter(|a| a.get_long().is_none() && a.get_short().is_none())
                .collect();
            let last_id = positional.last().map(|a| a.get_id().as_str());
            let args: Vec<serde_json::Value> = positional
                .iter()
                .map(|arg| {
                    let id = arg.get_id().as_str();
                    let kind = if (trailing && Some(id) == last_id) || id == "json" {
                        "json"
                    } else if id == "entity" {
                        "entity"
                    } else {
                        "string"
                    };
                    let mut arg_json = serde_json::json!({
                        "name": id,
                        "kind": kind,
                        "optional": !arg.is_required_set(),
                    });
                    if let Some(help) = arg.get_help() {
                        arg_json["help"] = serde_json::Value::String(help.to_string());
                    }
                    arg_json
                })
                .collect();
            let mut cmd_json = serde_json::json!({ "cmd": name, "args": args });
            if let Some(about) = cmd.get_about() {
                cmd_json["help"] = serde_json::Value::String(about.to_string());
            }
            cmd_json
        })
        .collect();
    let json = serde_json::to_string(&commands).unwrap_or_default();
    build_engine_api(&json);
}

/// Pings the JS watchdog each frame so it can detect a stalled engine loop.
fn engine_heartbeat_system(_js: NonSend<JsThread>) {
    engine_heartbeat();
}

fn update_text_focus(
    priorities: Res<InputPriorities>,
    mut prev: Local<bool>,
    _js: NonSend<JsThread>,
) {
    let focused = priorities.keyboard_claimed();
    if focused != *prev {
        *prev = focused;
        set_engine_text_focus(focused);
    }
}

pub fn update_winit_fps(config: Res<AppConfig>, mut winit: ResMut<WinitSettings>) {
    if config.is_changed() {
        let target = config.graphics.fps_target;
        let delay_micros = 1_000_000.0 / target as f32;
        winit.focused_mode = UpdateMode::Reactive {
            wait: std::time::Duration::from_micros((delay_micros) as u64),
            react_to_device_events: false,
            react_to_user_events: false,
            react_to_window_events: false,
        };
        winit.unfocused_mode = winit.focused_mode;
    }
}

/// Keeps the page url in step with the engine: the launch options with the live realm,
/// position, ui scene, portables and mode swapped in, sent whole so every param given at
/// launch (pulse server, imposter source, …) is retained alongside them.
fn update_url_params(
    player: Query<&GlobalTransform, With<PrimaryUser>>,
    current_realm: Res<CurrentRealm>,
    startup_scenes: Option<Res<StartupScenes>>,
    preview: Res<PreviewMode>,
    editor: Res<EditorMode>,
    mut prev: Local<Option<EngineRunOptions>>,
    _js: NonSend<JsThread>,
) {
    let parcel = vec3_to_parcel(player.single().map(|p| p.translation()).unwrap_or_default());
    let position = Some(format!("{},{}", parcel.x, parcel.y));
    let Some(server) = current_realm.about_url.strip_suffix("/about") else {
        return;
    };
    let (system_scene, portables) = if let Some(s) = startup_scenes {
        // the ui scene is the super-user scene inserted at index 0 — when one was given at all
        // (?systemScene=none boots with only regular startup scenes/portables)
        let ui_scene = s
            .scenes
            .first()
            .filter(|scene| scene.super_user)
            .map(|scene| scene.source.clone());
        let portables = s
            .scenes
            .iter()
            .skip(ui_scene.is_some() as usize)
            .map(|scene| scene.source.clone())
            .collect::<Vec<_>>();

        (
            ui_scene,
            (!portables.is_empty()).then(|| portables.join(";")),
        )
    } else {
        (None, None)
    };

    let launched = LAUNCH_OPTIONS.get().cloned().unwrap_or_default();
    let options = EngineRunOptions {
        launch: LaunchOptions {
            realm: Some(server.to_owned()),
            position,
            preview: preview.is_preview,
            ..launched.launch
        },
        client: ClientOptions {
            system_scene,
            // the default set is omitted so the canonical url stays clean (the page doesn't know it)
            portables: portables.filter(|p| p != system_api_types::web_params::DEFAULT_PORTABLES),
            editor: editor.0,
            ..launched.client
        },
    };

    if prev.as_ref() != Some(&options) {
        let json = serde_json::to_value(&options).unwrap_or_default();
        set_url_params(&json.to_string());
        *prev = Some(options);
    }
}

fn decentraland_serialized_app_config() -> AppConfig {
    INIT_DATA.get().cloned().unwrap_or_default()
}

fn decentraland_app_arguments(options: &EngineRunOptions) -> DecentralandArguments {
    let EngineRunOptions { launch, client } = options.clone();
    DecentralandArguments {
        launch,
        client,
        // wasm has no engine-managed HUD: the react page hosting the engine is the HUD
        hud: false,
        ..Default::default()
    }
}

pub struct PipelineHandler;

impl PipelineCompilationHandler for PipelineHandler {
    fn precreate_render_pipeline<'a>(
        &self,
        device: &'a RenderDevice,
        desc: &'a wgpu::RenderPipelineDescriptor,
    ) -> BoxedFuture<'a, ()> {
        Box::pin(async {
            allow_a_dummy_pipeline();
            let _ = device.create_render_pipeline(desc);
            if !last_pipeline_was_valid() {
                let _ = wasm_bindgen_futures::JsFuture::from(wait_for_async_pipelines()).await;
            }
        })
    }
}
