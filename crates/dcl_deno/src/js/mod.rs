use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};

use base64::{prelude::BASE64_URL_SAFE_NO_PAD, Engine};
use bevy::log::{debug, error, info_span};
use common::structs::GlobalCrdtStateUpdate;
use dcl::{
    interface::{crdt_context::CrdtContext, CrdtComponentInterfaces, CrdtStore},
    js::{
        engine::crdt_send_to_renderer, init_state, CommunicatedWithRenderer, CrdtSendsThisTick,
        KillFlag, SceneResponseSender, SuperUserScene,
    },
    RendererResponse, RpcCalls, SceneElapsedTime, SceneResourceCounters, SceneResponse,
};
use deno_core::{
    anyhow::anyhow,
    ascii_str,
    error::{generic_error, AnyError},
    include_js_files, op2, v8, Extension, JsRuntime, OpDecl, OpState, PollEventLoopOptions,
    RuntimeOptions,
};
use multihash_codetable::MultihashDigest;
use platform::project_directories;
use system_bridge::SystemApi;
use tokio::{sync::mpsc::UnboundedReceiver, time::timeout};

use ipfs::SceneJsFile;

#[cfg(feature = "inspect")]
use crate::js::inspector::InspectorServer;
use crate::VM_HANDLES;
#[cfg(feature = "inspect")]
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
#[cfg(not(feature = "inspect"))]
pub struct InspectorServer;

use self::{
    fetch::{FP, NP, TP},
    websocket::WebSocketPerms,
};

pub mod fetch;
#[cfg(feature = "inspect")]
pub mod inspector;
pub mod local_storage;
pub mod op_wrappers;
pub mod websocket;

pub fn create_runtime(
    inspect: bool,
    super_user: bool,
    storage_root: &str,
    preview: bool,
    kill_flag: KillFlag,
) -> (JsRuntime, Option<InspectorServer>) {
    // add fetch stack
    let net = deno_net::deno_net::init_ops_and_esm::<NP>(None, None);
    let web = deno_web::deno_web::init_ops_and_esm::<TP>(
        std::sync::Arc::new(deno_web::BlobStore::default()),
        None,
    );
    let webidl = deno_webidl::deno_webidl::init_ops_and_esm();
    let url = deno_url::deno_url::init_ops_and_esm();
    let console = deno_console::deno_console::init_ops_and_esm();
    let fetch = deno_fetch::deno_fetch::init_ops_and_esm::<FP>(deno_fetch::Options::default());
    let websocket = deno_websocket::deno_websocket::init_ops_and_esm::<WebSocketPerms>(
        "bevy-explorer".to_owned(),
        None,
        None,
    );

    let storage_digest = multihash_codetable::Code::Sha2_256.digest(storage_root.as_bytes());
    let storage_hash = BASE64_URL_SAFE_NO_PAD.encode(storage_digest.digest());
    let storage_folder = project_directories()
        .unwrap()
        .data_local_dir()
        .join("LocalStorage")
        .join(storage_hash);
    let webstorage = deno_webstorage::deno_webstorage::init_ops_and_esm(Some(storage_folder));

    let mut ops = vec![op_require(), op_log(), op_error()];

    let op_sets: [Vec<deno_core::OpDecl>; 13] = [
        op_wrappers::engine::ops(),
        op_wrappers::restricted_actions::ops(),
        op_wrappers::runtime::ops(),
        fetch::ops(),
        op_wrappers::portables::ops(),
        op_wrappers::user_identity::ops(),
        op_wrappers::player::ops(),
        op_wrappers::events::ops(),
        op_wrappers::comms::ops(),
        op_wrappers::testing::ops(),
        op_wrappers::ethereum_controller::ops(),
        op_wrappers::adaption_layer_helper::ops(),
        op_wrappers::system_api::ops(super_user),
    ];

    // add plugin registrations
    let mut op_map = HashMap::new();
    for set in op_sets {
        for op in &set {
            // explicitly record the ones we added so we can remove deno_fetch imposters
            op_map.insert(op.name, *op);
        }
        ops.extend(set);
    }

    let override_sets: [Vec<deno_core::OpDecl>; 3] = [
        fetch::override_ops(),
        websocket::override_ops(),
        local_storage::override_ops(),
    ];

    for set in override_sets {
        for op in set {
            // explicitly record the ones we added so we can remove deno_fetch imposters
            op_map.insert(op.name, op);
        }
    }

    let mut esm_files = include_js_files!(
        BevyExplorer
        dir "../dcl/src/js/modules",
    )
    .to_vec();

    esm_files.extend(include_js_files!(
        BevyExplorer
        dir "src/js/modules",
        "init.js",
    ));

    let ext = Extension {
        name: "decentraland",
        ops: ops.into(),
        esm_files: esm_files.into(),
        esm_entry_point: Some("ext:BevyExplorer/init.js"),
        middleware_fn: Some(Box::new(move |op: OpDecl| -> OpDecl {
            if let Some(custom_op) = op_map.get(&op.name) {
                debug!("replace: {}", op.name);
                op.with_implementation_from(custom_op)
            } else {
                debug!("default: {}", op.name);
                op
            }
        })),
        ..Default::default()
    };

    // Per-isolate V8 heap cap. Every scene runs in its own isolate on its own thread, so
    // a cap here bounds ONE scene's memory and stops a single runaway/hostile scene from
    // OOM-killing the shared sidecar (and every co-tenant scene with it). The near-limit
    // callback below terminates just that isolate instead of aborting the process.
    const MAX_SCENE_HEAP_BYTES: usize = 512 * 1024 * 1024;

    // create runtime
    #[allow(unused_mut)]
    let mut runtime = JsRuntime::new(RuntimeOptions {
        extensions: vec![
            webidl, url, console, web, net, fetch, websocket, webstorage, ext,
        ],
        inspector: inspect,
        create_params: Some(
            deno_core::v8::CreateParams::default().heap_limits(0, MAX_SCENE_HEAP_BYTES),
        ),
        ..Default::default()
    });

    // Deno extension permission objects. These must be in the op state from the moment the
    // runtime exists: the ops that consult them look the type up unconditionally, and a
    // missing type is itself a panic -- which across the V8 boundary aborts the process
    // rather than raising a JS error. Put here rather than at scene setup so no entry point
    // can forget them.
    {
        let state = runtime.op_state();
        let mut state = state.borrow_mut();
        state.put(TP);
        state.put(NP);
        state.put(WebSocketPerms { preview });
    }

    // On approaching the cap, terminate this isolate's execution so its JS unwinds and the
    // scene ends cleanly; raise the reported limit so V8 has headroom to run the
    // termination itself instead of hard-aborting the whole sidecar process.
    {
        let terminate_handle = runtime.v8_isolate().thread_safe_handle();
        let granted = std::sync::atomic::AtomicBool::new(false);
        runtime.add_near_heap_limit_callback(move |current, _initial| {
            let first_trip = !granted.swap(true, std::sync::atomic::Ordering::SeqCst);
            if first_trip {
                bevy::prelude::error!("scene exceeded its {MAX_SCENE_HEAP_BYTES}-byte heap cap; terminating the scene isolate");
            }
            // termination alone only unwinds the current js: the heap stays rooted by the
            // scene globals and the scene loop keeps ticking through uncaught errors, so it
            // would re-trip this callback forever. the kill flag makes the loop exit, which
            // drops the runtime and actually releases the heap.
            kill_flag.kill();
            terminate_handle.terminate_execution();
            if first_trip {
                // one-time margin so the termination unwind can run rather than hard-aborting
                current + 8 * 1024 * 1024
            } else {
                // kill already latched: stop re-granting, so a scene that keeps allocating
                // through termination can't ratchet the cap upward on every callback
                current
            }
        });
    }

    #[cfg(feature = "inspect")]
    if inspect {
        bevy::prelude::info!(
            "[{}] inspector attached",
            std::thread::current().name().unwrap()
        );
        let server = InspectorServer::new(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 9222),
            "bevy-explorer",
        );
        server.register_inspector("decentraland".to_owned(), &mut runtime, true);
        (runtime, Some(server))
    } else {
        (runtime, None)
    }

    #[cfg(not(feature = "inspect"))]
    if inspect {
        panic!("can't inspect without inspect feature")
    } else {
        (runtime, None)
    }
}

pub struct StorageRoot(pub String);

// main scene processing thread - constructs an isolate and runs the scene
#[allow(clippy::too_many_arguments)]
pub(crate) fn scene_thread(
    initial_crdt_store: CrdtStore,
    scene_context: CrdtContext,
    storage_root: String,
    scene_js: SceneJsFile,
    crdt_component_interfaces: CrdtComponentInterfaces,
    thread_sx: SceneResponseSender,
    thread_rx: UnboundedReceiver<RendererResponse>,
    global_update_receiver: tokio::sync::broadcast::Receiver<GlobalCrdtStateUpdate>,
    inspect: bool,
    super_user: Option<tokio::sync::mpsc::UnboundedSender<SystemApi>>,
    scene_origin: bevy::prelude::Vec3,
) {
    let scene_id = scene_context.scene_id;
    let preview = scene_context.preview;
    let kill_flag = KillFlag::default();
    let (mut runtime, inspector) = create_runtime(
        inspect,
        super_user.is_some(),
        &storage_root,
        preview,
        kill_flag.clone(),
    );

    // store handle
    let vm_handle = runtime.v8_isolate().thread_safe_handle();
    let mut guard = VM_HANDLES.lock().unwrap();
    guard.insert(scene_id, (vm_handle, kill_flag.clone()));
    drop(guard);

    let state = runtime.op_state();
    init_state(
        &mut *state.borrow_mut(),
        initial_crdt_store,
        scene_context,
        storage_root,
        scene_js,
        crdt_component_interfaces,
        thread_sx,
        thread_rx,
        global_update_receiver,
        super_user,
        scene_origin,
        kill_flag.clone(),
    );

    let span = info_span!("js startup").entered();
    state.borrow_mut().put(span);

    // store kill handle
    state
        .borrow_mut()
        .put(runtime.v8_isolate().thread_safe_handle());

    if inspector.is_some() {
        let _ = state
            .borrow_mut()
            .borrow_mut::<SceneResponseSender>()
            .try_send(SceneResponse::WaitingForInspector);

        runtime
            .inspector()
            .borrow_mut()
            .wait_for_session_and_break_on_next_statement();
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .enable_io()
        .build()
        .unwrap();

    // load module
    let script = rt.block_on(async {
        runtime.execute_script("<loader>", ascii_str!("require (\"~scene.js\")"))
    });

    let script = match script {
        Err(e) => {
            error!("[scene thread {scene_id:?}] script load error: {}", e);
            let _ = state
                .borrow_mut()
                .take::<SceneResponseSender>()
                .try_send(SceneResponse::Error(scene_id, format!("{e:?}")));
            return;
        }
        Ok(script) => script,
    };

    debug!(
        "[scene thread {scene_id:?}] post script execute, {} rpc calls",
        state.borrow().borrow::<RpcCalls>().len()
    );

    // send any initial rpc requests
    crdt_send_to_renderer(state.clone(), &[]);
    // the initial send drew on the tick's send allowance; give onStart a full one
    state.borrow_mut().try_take::<CrdtSendsThisTick>();

    // run startup function
    let run_start = thread_cpu_us();
    let result =
        rt.block_on(async { run_script(&mut runtime, &script, "onStart", |_| Vec::new()).await });
    state
        .borrow_mut()
        .borrow_mut::<SceneResourceCounters>()
        .run_us += thread_cpu_us().saturating_sub(run_start);

    debug!(
        "[scene thread {scene_id:?}] post startup, {} rpc calls",
        state.borrow().borrow::<RpcCalls>().len()
    );

    if let Err(e) = result {
        // ignore failure to send failure
        error!("[{scene_id:?}] onStart err: {e:?}");
        let _ = state
            .borrow_mut()
            .take::<SceneResponseSender>()
            .try_send(SceneResponse::Error(scene_id, format!("{e:?}")));
        return;
    }

    // Scenes aren't driven on a fixed timestep, so a slow frame (asset load, GC, a stalled
    // renderer round-trip) yields a huge dt. Cap it so dt-scaled scene logic (timers, animations)
    // is never handed a multi-second step. Mirrored by MAX_SCENE_DT_SECONDS in
    // deploy/web/sandbox_worker.js for the wasm runtime.
    const MAX_SCENE_DT: std::time::Duration = std::time::Duration::from_secs(1);

    let start_time = std::time::Instant::now();
    let mut prev_time = start_time;
    let mut elapsed;
    let mut reported_errors = 0;
    let mut last_heap_sample: Option<std::time::Instant> = None;
    loop {
        let now = std::time::Instant::now();
        let dt = now.saturating_duration_since(prev_time).min(MAX_SCENE_DT);
        elapsed = now.saturating_duration_since(start_time);
        prev_time = now;

        state
            .borrow_mut()
            .put(SceneElapsedTime(elapsed.as_secs_f32()));
        // tick boundary: reset the bounded per-tick send allowance
        state.borrow_mut().try_take::<CrdtSendsThisTick>();

        // heap gauges: sampling walks the isolate's spaces, so cap it at ~once per 5s
        if last_heap_sample.is_none_or(|at| now.saturating_duration_since(at).as_secs() >= 5) {
            last_heap_sample = Some(now);
            let mut heap = v8::HeapStatistics::default();
            runtime.v8_isolate().get_heap_statistics(&mut heap);
            let mut guard = state.borrow_mut();
            let counters = guard.borrow_mut::<SceneResourceCounters>();
            counters.heap_used = heap.used_heap_size() as u64;
            counters.heap_limit = heap.heap_size_limit() as u64;
        }

        // run the onUpdate function
        let run_start = thread_cpu_us();
        let result = rt.block_on(async {
            run_script(&mut runtime, &script, "onUpdate", |scope| {
                vec![v8::Number::new(scope, dt.as_secs_f64()).into()]
            })
            .await
        });
        state
            .borrow_mut()
            .borrow_mut::<SceneResourceCounters>()
            .run_us += thread_cpu_us().saturating_sub(run_start);

        // set cooperatively by the ops (renderer channel closed, policy kill) or
        // externally with terminate_execution (watchdog kill, heap cap); either way
        // exit instead of running another tick, which for a terminated scene would
        // re-enter a deterministic wedge
        if kill_flag.killed() {
            debug!("[{scene_id:?}] scene loop exiting");
            rt.block_on(async move {
                drop(runtime);
            });
            return;
        }

        if let Err(e) = result {
            reported_errors += 1;
            if reported_errors <= 10 {
                error!("[{scene_id:?}] uncaught error: {e:?}");
                if reported_errors == 10 {
                    error!("[{scene_id:?}] not logging any further uncaught errors.")
                }
            }

            // we no longer exit on uncaught `onUpdate` errors unless the scene failed to reach the renderer interface functions
            if reported_errors == 10
                && state
                    .borrow()
                    .try_borrow::<CommunicatedWithRenderer>()
                    .is_none()
            {
                error!(
                    "[{scene_id:?}] too many errors without renderer interaction: shutting down"
                );
                let _ = state
                    .borrow_mut()
                    .take::<SceneResponseSender>()
                    .try_send(SceneResponse::Error(scene_id, format!("{e:?}")));
                rt.block_on(async move {
                    drop(runtime);
                });
                return;
            }
        }

        state.borrow_mut().try_take::<CommunicatedWithRenderer>();
    }
}

/// Microseconds of CPU time consumed by the calling thread. The scene loop runs
/// lockstep with the renderer (run_script blocks on the crdt round-trip inside a
/// tick), so wall time would count idle waiting as script time; thread CPU time
/// only advances while JS actually executes. Non-unix targets fall back to wall
/// time.
fn thread_cpu_us() -> u64 {
    #[cfg(unix)]
    {
        let mut ts = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY: ts is a valid, writable timespec
        if unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) } == 0 {
            (ts.tv_sec as u64) * 1_000_000 + (ts.tv_nsec as u64) / 1_000
        } else {
            0
        }
    }
    #[cfg(not(unix))]
    {
        use std::sync::OnceLock;
        static START: OnceLock<std::time::Instant> = OnceLock::new();
        START
            .get_or_init(std::time::Instant::now)
            .elapsed()
            .as_micros() as u64
    }
}

// helper to setup, acquire, run and return results from a script function
async fn run_script(
    runtime: &mut JsRuntime,
    script: &v8::Global<v8::Value>,
    fn_name: &str,
    arg_fn: impl for<'a> Fn(&mut v8::HandleScope<'a>) -> Vec<v8::Local<'a, v8::Value>>,
) -> Result<(), AnyError> {
    // set up scene i/o
    let promise = {
        let scope = &mut runtime.handle_scope();
        let script_this = v8::Local::new(scope, script.clone());
        // get module
        let script = v8::Local::<v8::Object>::try_from(script_this).unwrap();

        // get function
        let target_function =
            v8::String::new_from_utf8(scope, fn_name.as_bytes(), v8::NewStringType::Internalized)
                .unwrap();
        let Some(target_function) = script.get(scope, target_function.into()) else {
            // function not define, is that an error ?
            // debug!("{fn_name} is not defined");
            return Err(AnyError::msg(format!("{fn_name} is not defined")));
        };
        let Ok(target_function) = v8::Local::<v8::Function>::try_from(target_function) else {
            // error!("{fn_name} is not a function");
            return Err(AnyError::msg(format!("{fn_name} is not a function")));
        };

        // get args
        let args = arg_fn(scope);

        // call
        let res = target_function.call(scope, script_this, &args);
        let Some(res) = res else {
            // error!("{fn_name} did not return a promise");
            return Err(AnyError::msg(format!("{fn_name} did not return a promise")));
        };

        drop(args);
        v8::Global::new(scope, res)
    };

    let f = runtime.resolve(promise);

    let result = if true {
        runtime
            .with_event_loop_promise(f, PollEventLoopOptions::default())
            .await
            .map(|_| ())
    } else {
        timeout(
            Duration::from_secs(30),
            runtime.with_event_loop_promise(f, PollEventLoopOptions::default()),
        )
        .await
        .map_err(|_| anyhow!("script timed out"))?
        .map(|_| ())
    };

    result
}

// synchronously returns a string containing JS code from the file system
#[op2]
#[string]
fn op_require(
    state: Rc<RefCell<OpState>>,
    #[string] module_spec: String,
) -> Result<String, deno_core::error::AnyError> {
    debug!("require(\"{module_spec}\")");

    match module_spec.as_str() {
        // user module load
        "~scene.js" => Ok(state.borrow().borrow::<SceneJsFile>().0.as_ref().clone()),
        // system api (only allowed for su scene)
        "~system/BevyExplorerApi" => {
            if state.borrow().try_borrow::<SuperUserScene>().is_some() {
                Ok(include_str!("../../../dcl/src/js/modules/SystemApi.js").to_owned())
            } else {
                Err(generic_error(format!(
                    "invalid module request `{module_spec}`"
                )))
            }
        }
        // core module load
        "~system/CommunicationsController" => {
            Ok(include_str!("../../../dcl/src/js/modules/CommunicationsController.js").to_owned())
        }
        "~system/CommsApi" => {
            Ok(include_str!("../../../dcl/src/js/modules/CommsApi.js").to_owned())
        }
        "~system/EngineApi" => {
            Ok(include_str!("../../../dcl/src/js/modules/EngineApi.js").to_owned())
        }
        "~system/EnvironmentApi" => {
            Ok(include_str!("../../../dcl/src/js/modules/EnvironmentApi.js").to_owned())
        }
        "~system/EthereumController" => {
            Ok(include_str!("../../../dcl/src/js/modules/EthereumController.js").to_owned())
        }
        "~system/Players" => Ok(include_str!("../../../dcl/src/js/modules/Players.js").to_owned()),
        "~system/PortableExperiences" => {
            Ok(include_str!("../../../dcl/src/js/modules/PortableExperiences.js").to_owned())
        }
        "~system/RestrictedActions" => {
            Ok(include_str!("../../../dcl/src/js/modules/RestrictedActions.js").to_owned())
        }
        "~system/Runtime" => Ok(include_str!("../../../dcl/src/js/modules/Runtime.js").to_owned()),
        "~system/Scene" => Ok(include_str!("../../../dcl/src/js/modules/Scene.js").to_owned()),
        "~system/SignedFetch" => {
            Ok(include_str!("../../../dcl/src/js/modules/SignedFetch.js").to_owned())
        }
        "~system/Testing" => Ok(include_str!("../../../dcl/src/js/modules/Testing.js").to_owned()),
        "~system/UserActionModule" => {
            Ok(include_str!("../../../dcl/src/js/modules/UserActionModule.js").to_owned())
        }
        "~system/UserIdentity" => {
            Ok(include_str!("../../../dcl/src/js/modules/UserIdentity.js").to_owned())
        }
        "~system/AdaptationLayerHelper" => {
            Ok(include_str!("../../../dcl/src/js/modules/AdaptationLayerHelper.js").to_owned())
        }
        _ => Err(generic_error(format!(
            "invalid module request `{module_spec}`"
        ))),
    }
}

#[op2(fast)]
fn op_log(state: Rc<RefCell<OpState>>, #[string] message: String) {
    dcl::js::op_log(state, message);
}

#[op2(fast)]
fn op_error(state: Rc<RefCell<OpState>>, #[string] message: String) {
    dcl::js::op_error(state, message);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use dcl::{interface::crdt_context::CrdtContext, SceneId};
    use deno_core::ascii_str;
    use ipfs::SceneJsFile;

    use super::create_runtime;

    fn context(is_server: bool) -> CrdtContext {
        CrdtContext::new(
            SceneId(bevy::prelude::Entity::from_raw(0)),
            "hash".to_owned(),
            "title".to_owned(),
            false,
            false,
            is_server,
        )
    }

    /// Boot a scene runtime and evaluate `scene_js` exactly the way a deployed scene bundle
    /// is evaluated (`op_require("~scene.js")` -> `evalContext`). A `throw` in the scene
    /// surfaces as `Err`.
    fn run_scene(is_server: bool, scene_js: &str) -> Result<(), String> {
        let (mut runtime, _) =
            create_runtime(false, false, "test-scene-realm", false, Default::default());
        {
            let state = runtime.op_state();
            let mut state = state.borrow_mut();
            state.put(context(is_server));
            state.put(SceneJsFile(Arc::new(scene_js.to_owned())));
        }
        runtime
            .execute_script("<test>", ascii_str!("require(\"~scene.js\")"))
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// The scene bundle shares a realm with the runtime, so anything left on the global
    /// object is scene-reachable -- and `Deno.core.ops` is every op with none of the checks
    /// the `~system/*` wrappers apply. This is the property, not an implementation detail.
    #[test]
    fn scene_cannot_reach_the_ops_table_on_the_server() {
        run_scene(
            true,
            r#"
            if (typeof Deno !== "undefined") { throw new Error("`Deno` is reachable"); }
            if (typeof __bootstrap !== "undefined") { throw new Error("`__bootstrap` is reachable"); }
            if (typeof __infra !== "undefined") { throw new Error("`__infra` is reachable"); }
            // the global object itself, reached the indirect way
            const g = Function("return this")();
            if (g.Deno !== undefined) { throw new Error("`Deno` is reachable via globalThis"); }
            if (g.__bootstrap !== undefined) { throw new Error("`__bootstrap` is reachable via globalThis"); }
            "#,
        )
        .unwrap();
    }

    /// ...while the modules the scene is *supposed* to use still get it, via the `Deno`
    /// parameter `require` injects. Required from scene code, i.e. after the seal.
    #[test]
    fn system_modules_still_work_after_sealing() {
        run_scene(
            true,
            r#"
            const engine = require("~system/EngineApi");
            if (typeof engine.crdtSendToRenderer !== "function") {
                throw new Error("EngineApi did not load");
            }
            if (typeof require("~system/Runtime").readFile !== "function") {
                throw new Error("Runtime did not load");
            }
            "#,
        )
        .unwrap();
    }

    /// The seal is scoped to the authoritative server on purpose: the desktop client runs on
    /// the user's own machine, and narrowing the change keeps preview and every deployed
    /// scene behaving exactly as they do today. Pinned so the scoping is a decision rather
    /// than something that drifts.
    #[test]
    fn client_realm_is_left_alone() {
        run_scene(
            false,
            r#"if (typeof Deno === "undefined") { throw new Error("client behaviour changed"); }"#,
        )
        .unwrap();
    }

    /// `deno_net`'s ops are registered, unwrapped, and reachable from any scene that gets at
    /// the ops table. They used to `panic!()` in the permission check -- across the V8
    /// boundary that is `panic_cannot_unwind`, i.e. SIGABRT, and on the client one sidecar
    /// hosts every scene and losing it exits the engine. It must be an ordinary JS error.
    #[test]
    fn raw_socket_ops_raise_instead_of_aborting() {
        let err = run_scene(
            false,
            r#"Deno.core.ops.op_net_listen_tcp({ hostname: "127.0.0.1", port: 9999, transport: "tcp" }, false, false)"#,
        )
        .expect_err("raw socket access must be refused");
        assert!(err.contains("raw socket access"), "unexpected error: {err}");
    }

    /// A scene-supplied proxy is dialled instead of the URL host, so it walks straight past
    /// the per-request `assert_public_url` check -- the check only ever sees the URL. On the
    /// authoritative server the option must be refused outright.
    #[test]
    fn server_refuses_a_scene_supplied_proxy() {
        let (mut runtime, _) = create_runtime(
            false,
            false,
            "test-custom-client",
            false,
            Default::default(),
        );
        runtime.op_state().borrow_mut().put(context(true));
        let err = runtime
            .execute_script(
                "<test>",
                ascii_str!(
                    r#"Deno.core.ops.op_fetch_custom_client({ caCerts: [], proxy: { url: "http://169.254.169.254:80" } })"#
                ),
            )
            .expect_err("proxy must be refused on the authoritative server")
            .to_string();
        assert!(err.contains("proxy"), "unexpected error: {err}");
    }

    /// Same for a scene-supplied trust root: it would make the scene a CA for the server's
    /// own outbound TLS.
    #[test]
    fn server_refuses_scene_supplied_ca_certs() {
        let (mut runtime, _) = create_runtime(
            false,
            false,
            "test-custom-client-ca",
            false,
            Default::default(),
        );
        runtime.op_state().borrow_mut().put(context(true));
        let err = runtime
            .execute_script(
                "<test>",
                ascii_str!(
                    r#"Deno.core.ops.op_fetch_custom_client({ caCerts: ["-----BEGIN CERTIFICATE-----"] })"#
                ),
            )
            .expect_err("ca_certs must be refused on the authoritative server")
            .to_string();
        assert!(err.contains("root certificates"), "unexpected error: {err}");
    }

    /// The desktop/web client runs on the user's own machine and keeps the old behaviour --
    /// this restriction is server-only.
    #[test]
    fn client_still_allows_custom_clients() {
        let (mut runtime, _) = create_runtime(
            false,
            false,
            "test-custom-client-clientmode",
            false,
            Default::default(),
        );
        runtime.op_state().borrow_mut().put(context(false));
        runtime
            .execute_script(
                "<test>",
                ascii_str!(
                    r#"Deno.core.ops.op_fetch_custom_client({ caCerts: [], proxy: { url: "http://localhost:8080" } })"#
                ),
            )
            .expect("client mode must keep working");
    }

    /// Drive one scene `fetch` against a local server that answers `/redirect` with a 302 to
    /// `/target` on a second listener, and `/target` with a 200. Returns `"<status> <body>"`.
    /// Loopback needs `preview`; the fetch permission request that `op_fetch_send` parks on is
    /// granted from a plain thread, as the renderer would (`RpcResultSender::send` blocks).
    fn fetch_redirect_outcome(is_server: bool) -> String {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::mpsc;

        let listeners = [
            TcpListener::bind("127.0.0.1:0").unwrap(),
            TcpListener::bind("127.0.0.1:0").unwrap(),
        ];
        let ports = listeners.each_ref().map(|l| l.local_addr().unwrap().port());
        for listener in listeners {
            let target_port = ports[1];
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let mut stream = stream.unwrap();
                    let mut buf = [0u8; 4096];
                    let n = stream.read(&mut buf).unwrap();
                    let path = String::from_utf8_lossy(&buf[..n])
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or("/")
                        .to_owned();
                    let response = if path == "/redirect" {
                        format!("HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{target_port}/target\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    } else {
                        "HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\narrived"
                            .to_owned()
                    };
                    stream.write_all(response.as_bytes()).unwrap();
                }
            });
        }

        let (mut runtime, _) = create_runtime(
            false,
            false,
            "test-fetch-redirect",
            true,
            Default::default(),
        );
        {
            let state = runtime.op_state();
            let mut state = state.borrow_mut();
            state.put(CrdtContext::new(
                SceneId(bevy::prelude::Entity::from_raw(0)),
                "hash".to_owned(),
                "title".to_owned(),
                false,
                true,
                is_server,
            ));
            state.put(dcl::SceneResourceCounters::default());
            state.put(dcl::RpcCalls::default());
            state.put(Vec::<dcl::SceneLogMessage>::default());
            state.put(dcl::SceneElapsedTime(0.0));
        }

        let (grant_sx, grant_rx) = mpsc::channel::<common::rpc::RpcResultSender<bool>>();
        std::thread::spawn(move || {
            for sender in grant_rx {
                sender.send(true);
            }
        });

        let js = format!(
            r#"globalThis.__outcome = fetch("http://127.0.0.1:{}/redirect")
                .then(async (r) => `${{r.status}} ${{await r.text()}}`, (e) => `error ${{e}}`)
                .then((s) => globalThis.__outcome = s);"#,
            ports[0]
        );
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            runtime.execute_script("<test>", js).unwrap();
            let op_state = runtime.op_state();
            let grant_permissions = async {
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    let calls =
                        std::mem::take(&mut **op_state.borrow_mut().borrow_mut::<dcl::RpcCalls>());
                    for call in calls {
                        if let common::rpc::RpcCall::RequestGenericPermission { response, .. } =
                            call
                        {
                            grant_sx.send(response).unwrap();
                        }
                    }
                }
            };
            tokio::select! {
                r = tokio::time::timeout(
                    std::time::Duration::from_secs(10),
                    runtime.run_event_loop(deno_core::PollEventLoopOptions::default()),
                ) => r.expect("fetch did not settle").unwrap(),
                _ = grant_permissions => unreachable!(),
            }
        });
        let outcome = runtime
            .execute_script("<test>", ascii_str!("String(globalThis.__outcome)"))
            .unwrap();
        let scope = &mut runtime.handle_scope();
        deno_core::v8::Local::new(scope, outcome).to_rust_string_lossy(scope)
    }

    /// The server never auto-follows a redirect: the scene gets the 3xx back. (Following
    /// would also need the `URL` global, which this runtime does not install -- pinned here
    /// so the outcome is the policy, not that accident.)
    #[test]
    fn server_hands_redirects_back_to_the_scene() {
        let outcome = fetch_redirect_outcome(true);
        assert!(outcome.starts_with("302 "), "unexpected outcome: {outcome}");
    }

    /// The client keeps following, like the browser does.
    #[test]
    fn client_follows_redirects() {
        assert_eq!(fetch_redirect_outcome(false), "200 arrived");
    }
}
