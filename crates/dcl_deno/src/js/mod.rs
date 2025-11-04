use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::mpsc::SyncSender};

use base64::{prelude::BASE64_URL_SAFE_NO_PAD, Engine};
use bevy::log::{debug, error, info_span};
use common::structs::MicState;
use dcl::{
    interface::CrdtComponentInterfaces,
    js::{
        engine::crdt_send_to_renderer, init_state, CommunicatedWithRenderer, ShuttingDown,
        SuperUserScene,
    },
    RendererResponse, RpcCalls, SceneElapsedTime, SceneId, SceneResponse,
};
use deno_core::{
    ascii_str,
    error::{generic_error, AnyError},
    include_js_files, op2, v8, Extension, JsRuntime, OpDecl, OpState, PollEventLoopOptions,
    RuntimeOptions,
};
use multihash_codetable::MultihashDigest;
use platform::project_directories;
use system_bridge::SystemApi;
use tokio::sync::mpsc::Receiver;

use ipfs::{IpfsResource, SceneJsFile};
use wallet::Wallet;

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

    // create runtime
    #[allow(unused_mut)]
    let mut runtime = JsRuntime::new(RuntimeOptions {
        extensions: vec![
            webidl, url, console, web, net, fetch, websocket, webstorage, ext,
        ],
        inspector: inspect,
        ..Default::default()
    });

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
    scene_hash: String,
    scene_id: SceneId,
    storage_root: String,
    scene_js: SceneJsFile,
    crdt_component_interfaces: CrdtComponentInterfaces,
    thread_sx: SyncSender<SceneResponse>,
    thread_rx: Receiver<RendererResponse>,
    global_update_receiver: tokio::sync::broadcast::Receiver<Vec<u8>>,
    ipfs: IpfsResource,
    wallet: Wallet,
    mic: MicState,
    inspect: bool,
    testing: bool,
    preview: bool,
    super_user: Option<tokio::sync::mpsc::UnboundedSender<SystemApi>>,
) {
    let (mut runtime, inspector) = create_runtime(inspect, super_user.is_some(), &storage_root);

    // store handle
    let vm_handle = runtime.v8_isolate().thread_safe_handle();
    let mut guard = VM_HANDLES.lock().unwrap();
    guard.insert(scene_id, vm_handle);
    drop(guard);

    let state = runtime.op_state();
    init_state(
        &mut *state.borrow_mut(),
        scene_hash,
        scene_id,
        storage_root,
        scene_js,
        crdt_component_interfaces,
        thread_sx,
        thread_rx,
        global_update_receiver,
        ipfs,
        wallet,
        mic,
        inspect,
        testing,
        preview,
        super_user,
    );

    // store deno permission objects
    state.borrow_mut().put(TP);

    let span = info_span!("js startup").entered();
    state.borrow_mut().put(span);

    // store kill handle
    state
        .borrow_mut()
        .put(runtime.v8_isolate().thread_safe_handle());

    // store websocket permissions object
    state.borrow_mut().put(WebSocketPerms { preview });

    if inspector.is_some() {
        let _ = state
            .borrow_mut()
            .borrow_mut::<SyncSender<SceneResponse>>()
            .send(SceneResponse::WaitingForInspector);

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
                .take::<SyncSender<SceneResponse>>()
                .send(SceneResponse::Error(scene_id, format!("{e:?}")));
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

    // run startup function
    let result =
        rt.block_on(async { run_script(&mut runtime, &script, "onStart", |_| Vec::new()).await });

    debug!(
        "[scene thread {scene_id:?}] post startup, {} rpc calls",
        state.borrow().borrow::<RpcCalls>().len()
    );

    if let Err(e) = result {
        // ignore failure to send failure
        error!("[{scene_id:?}] onStart err: {e:?}");
        let _ = state
            .borrow_mut()
            .take::<SyncSender<SceneResponse>>()
            .send(SceneResponse::Error(scene_id, format!("{e:?}")));
        return;
    }

    let start_time = std::time::Instant::now();
    let mut prev_time = start_time;
    let mut elapsed;
    let mut reported_errors = 0;
    loop {
        let now = std::time::Instant::now();
        let dt = now.saturating_duration_since(prev_time);
        elapsed = now.saturating_duration_since(start_time);
        prev_time = now;

        state
            .borrow_mut()
            .put(SceneElapsedTime(elapsed.as_secs_f32()));

        // run the onUpdate function
        let result = rt.block_on(async {
            run_script(&mut runtime, &script, "onUpdate", |scope| {
                vec![v8::Number::new(scope, dt.as_secs_f64()).into()]
            })
            .await
        });

        if state.borrow().try_borrow::<ShuttingDown>().is_some() {
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
                    .take::<SyncSender<SceneResponse>>()
                    .send(SceneResponse::Error(scene_id, format!("{e:?}")));
                rt.block_on(async move {
                    drop(runtime);
                });
                return;
            }
        }

        state.borrow_mut().try_take::<CommunicatedWithRenderer>();
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
    runtime
        .with_event_loop_promise(f, PollEventLoopOptions::default())
        .await
        .map(|_| ())
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
