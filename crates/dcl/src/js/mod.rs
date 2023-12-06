use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::mpsc::SyncSender};

use bevy::utils::tracing::{debug, error, info_span};
use deno_core::{
    anyhow::anyhow,
    ascii_str,
    error::{generic_error, AnyError},
    include_js_files, op, v8, Extension, JsRuntime, Op, OpDecl, OpState, RuntimeOptions,
};
use deno_websocket::WebSocketPermissions;
use tokio::sync::mpsc::Receiver;

use ipfs::{IpfsResource, SceneJsFile};
use wallet::Wallet;

use crate::RpcCalls;

#[cfg(feature = "inspect")]
use crate::js::inspector::InspectorServer;
#[cfg(feature = "inspect")]
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
#[cfg(not(feature = "inspect"))]
pub struct InspectorServer;

use self::fetch::{FP, TP};

use super::{
    interface::{crdt_context::CrdtContext, CrdtComponentInterfaces, CrdtStore},
    RendererResponse, SceneElapsedTime, SceneId, SceneLogLevel, SceneLogMessage, SceneResponse,
    VM_HANDLES,
};

pub mod engine;
pub mod fetch;
pub mod portables;
pub mod restricted_actions;
pub mod runtime;
pub mod user_identity;

pub mod comms;
pub mod events;
#[cfg(feature = "inspect")]
pub mod inspector;
pub mod player;

// marker to indicate shutdown has been triggered
pub struct ShuttingDown;

pub struct RendererStore(pub CrdtStore);

pub struct WebSocketPerms;

impl WebSocketPermissions for WebSocketPerms {
    fn check_net_url(
        &mut self,
        url: &deno_core::url::Url,
        _api_name: &str,
    ) -> Result<(), AnyError> {
        // TODO scene permissions

        // must use `wss`
        if url.scheme() == "wss" {
            Ok(())
        } else {
            Err(anyhow!("URL scheme must be `wss`"))
        }
    }
}

pub fn create_runtime(init: bool, inspect: bool) -> (JsRuntime, Option<InspectorServer>) {
    // add fetch stack
    let web = deno_web::deno_web::init_ops_and_esm::<TP>(
        std::sync::Arc::new(deno_web::BlobStore::default()),
        None,
    );
    let webidl = deno_webidl::deno_webidl::init_ops_and_esm();
    let url = deno_url::deno_url::init_ops_and_esm();
    let console = deno_console::deno_console::init_ops_and_esm();
    let fetch = deno_fetch::deno_fetch::init_js_only::<FP>();
    let websocket = deno_websocket::deno_websocket::init_ops_and_esm::<WebSocketPerms>(
        "bevy-explorer".to_owned(),
        None,
        None,
    );

    let mut ops = vec![op_require::DECL, op_log::DECL, op_error::DECL];

    let op_sets: [Vec<deno_core::OpDecl>; 9] = [
        engine::ops(),
        restricted_actions::ops(),
        runtime::ops(),
        fetch::ops(),
        portables::ops(),
        user_identity::ops(),
        player::ops(),
        events::ops(),
        comms::ops(),
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

    let override_sets: [Vec<deno_core::OpDecl>; 1] = [fetch::override_ops()];

    for set in override_sets {
        for op in set {
            // explicitly record the ones we added so we can remove deno_fetch imposters
            op_map.insert(op.name, op);
        }
    }

    let ext = Extension {
        name: "decentraland",
        deps: &["deno_fetch"],
        ops: ops.into(),
        esm_files: include_js_files!(
            BevyExplorer
            dir "src/js/modules",
            "init.js",
        )
        .to_vec()
        .into(),
        esm_entry_point: Some("ext:BevyExplorer/init.js"),
        middleware_fn: Some(Box::new(move |op: OpDecl| -> OpDecl {
            if let Some(custom_op) = op_map.get(&op.name) {
                debug!("replace: {}", op.name);
                op.with_implementation_from(custom_op)
            } else {
                op
            }
        })),
        ..Default::default()
    };

    // create runtime
    #[allow(unused_mut)]
    let mut runtime = JsRuntime::new(RuntimeOptions {
        v8_platform: if init {
            v8::Platform::new(1, false).make_shared().into()
        } else {
            None
        },
        extensions: vec![webidl, url, console, web, fetch, websocket, ext],
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

// main scene processing thread - constructs an isolate and runs the scene
#[allow(clippy::too_many_arguments)]
pub(crate) fn scene_thread(
    scene_hash: String,
    scene_id: SceneId,
    scene_js: SceneJsFile,
    crdt_component_interfaces: CrdtComponentInterfaces,
    thread_sx: SyncSender<SceneResponse>,
    thread_rx: Receiver<RendererResponse>,
    global_update_receiver: tokio::sync::broadcast::Receiver<Vec<u8>>,
    ipfs: IpfsResource,
    wallet: Wallet,
    inspect: bool,
) {
    let scene_context = CrdtContext::new(scene_id, scene_hash);
    let (mut runtime, inspector) = create_runtime(false, inspect);

    // store handle
    let vm_handle = runtime.v8_isolate().thread_safe_handle();
    let mut guard = VM_HANDLES.lock().unwrap();
    guard.insert(scene_id, vm_handle);
    drop(guard);

    let state = runtime.op_state();

    // store scene detail in the runtime state
    state.borrow_mut().put(scene_context);
    state.borrow_mut().put(scene_js);

    // store the component writers
    state.borrow_mut().put(crdt_component_interfaces);

    // store channels
    state.borrow_mut().put(thread_sx);
    state.borrow_mut().put(thread_rx);
    state.borrow_mut().put(global_update_receiver);

    // store asset server and wallet
    state.borrow_mut().put(ipfs);
    state.borrow_mut().put(wallet);

    // store crdt outbound state and event queue
    state.borrow_mut().put(CrdtStore::default());
    state.borrow_mut().put(RpcCalls::default());
    // and renderer incoming state
    state.borrow_mut().put(RendererStore(CrdtStore::default()));

    // store log output and initial elapsed of zero
    state.borrow_mut().put(Vec::<SceneLogMessage>::default());
    state.borrow_mut().put(SceneElapsedTime(0.0));

    let span = info_span!("js startup").entered();
    state.borrow_mut().put(span);

    // store kill handle
    state
        .borrow_mut()
        .put(runtime.v8_isolate().thread_safe_handle());

    // store websocket permissions object
    state.borrow_mut().put(WebSocketPerms);

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

    // load module
    let script = runtime.execute_script("<loader>", ascii_str!("require (\"~scene.js\")"));

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

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .enable_io()
        .build()
        .unwrap();

    // run startup function
    let result =
        rt.block_on(async { run_script(&mut runtime, &script, "onStart", |_| Vec::new()).await });

    if let Err(e) = result {
        // ignore failure to send failure
        let _ = state
            .borrow_mut()
            .take::<SyncSender<SceneResponse>>()
            .send(SceneResponse::Error(scene_id, format!("{e:?}")));
        return;
    }

    let start_time = std::time::Instant::now();
    let mut prev_time = start_time;
    let mut elapsed;
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
            return;
        }

        if let Err(e) = result {
            let _ = state
                .borrow_mut()
                .take::<SyncSender<SceneResponse>>()
                .send(SceneResponse::Error(scene_id, format!("{e:?}")));
            return;
        }
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

    let f = runtime.resolve_value(promise);
    f.await.map(|_| ())
}

// synchronously returns a string containing JS code from the file system
#[op(v8)]
fn op_require(
    state: Rc<RefCell<OpState>>,
    module_spec: String,
) -> Result<String, deno_core::error::AnyError> {
    debug!("require(\"{module_spec}\")");

    match module_spec.as_str() {
        // user module load
        "~scene.js" => Ok(state.borrow().borrow::<SceneJsFile>().0.as_ref().clone()),
        // core module load
        "~system/CommunicationsController" => {
            Ok(include_str!("modules/CommunicationsController.js").to_owned())
        }
        "~system/CommsApi" => {
            Ok(include_str!("modules/CommsApi.js").to_owned())
        }
        "~system/EngineApi" => Ok(include_str!("modules/EngineApi.js").to_owned()),
        "~system/EnvironmentApi" => Ok(include_str!("modules/EnvironmentApi.js").to_owned()),
        "~system/EthereumController" => {
            Ok(include_str!("modules/EthereumController.js").to_owned())
        }
        "~system/Players" => Ok(include_str!("modules/Players.js").to_owned()),
        "~system/PortableExperiences" => {
            Ok(include_str!("modules/PortableExperiences.js").to_owned())
        }
        "~system/RestrictedActions" => Ok(include_str!("modules/RestrictedActions.js").to_owned()),
        "~system/Runtime" => Ok(include_str!("modules/Runtime.js").to_owned()),
        "~system/Scene" => Ok(include_str!("modules/Scene.js").to_owned()),
        "~system/SignedFetch" => Ok(include_str!("modules/SignedFetch.js").to_owned()),
        "~system/Testing" => Ok(include_str!("modules/Testing.js").to_owned()),
        "~system/UserActionModule" => Ok(include_str!("modules/UserActionModule.js").to_owned()),
        "~system/UserIdentity" => Ok(include_str!("modules/UserIdentity.js").to_owned()),
        _ => Err(generic_error(format!(
            "invalid module request `{module_spec}`"
        ))),
    }
}

#[op(v8)]
fn op_log(state: Rc<RefCell<OpState>>, message: String) {
    let time = state.borrow().borrow::<SceneElapsedTime>().0;
    // info!(message);
    state
        .borrow_mut()
        .borrow_mut::<Vec<SceneLogMessage>>()
        .push(SceneLogMessage {
            timestamp: time as f64,
            level: SceneLogLevel::Log,
            message,
        })
}

#[op(v8)]
fn op_error(state: Rc<RefCell<OpState>>, message: String) {
    let time = state.borrow().borrow::<SceneElapsedTime>().0;
    state
        .borrow_mut()
        .borrow_mut::<Vec<SceneLogMessage>>()
        .push(SceneLogMessage {
            timestamp: time as f64,
            level: SceneLogLevel::SceneError,
            message,
        })
}
