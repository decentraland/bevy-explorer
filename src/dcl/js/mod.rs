use std::{cell::RefCell, rc::Rc, sync::mpsc::SyncSender, time::Duration};

use bevy::prelude::{debug, error, info_span};
use deno_core::{
    error::{generic_error, AnyError},
    include_js_files, op, v8, Extension, JsRuntime, OpState, RuntimeOptions,
};
use tokio::sync::mpsc::Receiver;

use self::context::SceneSceneContext;
use crate::SceneDefinition;

use super::{
    interface::{CrdtComponentInterfaces, CrdtStore},
    RendererResponse, SceneId, SceneResponse, VM_HANDLES,
};

pub mod context;
pub mod engine;

// marker to indicate shutdown has been triggered
pub struct ShuttingDown;

// main scene processing thread - constructs an isolate and runs the scene
pub(crate) fn scene_thread(
    scene_id: SceneId,
    scene_definition: SceneDefinition,
    crdt_component_interfaces: CrdtComponentInterfaces,
    thread_sx: SyncSender<SceneResponse>,
    thread_rx: Receiver<RendererResponse>,
) {
    let scene_context = SceneSceneContext::new(scene_definition, scene_id);

    // create an extension referencing our native functions and JS initialisation scripts
    // TODO: to make this more generic for multiple modules we could use
    // https://crates.io/crates/inventory or similar
    let ext = Extension::builder("decentraland")
        // add require operation
        .ops(vec![op_require::decl()])
        // add plugin registrations
        .ops(engine::ops())
        // set startup JS script
        .js(include_js_files!(
            prefix "example:init",
            "init.js",
        ))
        // remove core deno ops that are not required
        .middleware(|op| {
            const ALLOW: [&str; 4] = [
                "op_print",
                "op_eval_context",
                "op_require",
                "op_crdt_send_to_renderer",
            ];
            if ALLOW.contains(&op.name) {
                op
            } else {
                debug!("deny: {}", op.name);
                // op.disable()
                op
            }
        })
        .build();

    // create runtime
    let mut runtime = JsRuntime::new(RuntimeOptions {
        v8_platform: v8::Platform::new(1, false).make_shared().into(),
        extensions_with_js: vec![ext],
        ..Default::default()
    });

    // store handle
    let vm_handle = runtime.v8_isolate().thread_safe_handle();
    let mut guard = VM_HANDLES.lock().unwrap();
    guard.insert(scene_id, vm_handle);
    drop(guard);

    let state = runtime.op_state();

    // store scene detail in the runtime state
    state.borrow_mut().put(scene_context);

    // store the component writers
    state.borrow_mut().put(crdt_component_interfaces);

    // store channels
    state.borrow_mut().put(thread_sx);
    state.borrow_mut().put(thread_rx);

    // store crdt state
    state.borrow_mut().put(CrdtStore::default());

    // store kill handle
    state
        .borrow_mut()
        .put(runtime.v8_isolate().thread_safe_handle());

    // load module
    let script = runtime.execute_script("<loader>", "require (\"index.js\")");

    let script = match script {
        Err(e) => {
            error!("script load error: {}", e);
            return;
        }
        Ok(script) => script,
    };

    // run startup function
    let result = run_script(&mut runtime, &script, "onStart", (), |_| Vec::new());

    if let Err(e) = result {
        // ignore failure to send failure
        let _ = state
            .borrow_mut()
            .take::<SyncSender<SceneResponse>>()
            .send(SceneResponse::Error(scene_id, format!("{e:?}")));
        return;
    }

    let start_time = std::time::SystemTime::now();
    let mut elapsed = Duration::default();
    loop {
        let dt = std::time::SystemTime::now()
            .duration_since(start_time)
            .unwrap_or(elapsed)
            - elapsed;
        elapsed += dt;

        // run the onUpdate function
        let result = run_script(&mut runtime, &script, "onUpdate", (), |scope| {
            vec![v8::Number::new(scope, dt.as_secs_f64()).into()]
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
fn run_script(
    runtime: &mut JsRuntime,
    script: &v8::Global<v8::Value>,
    fn_name: &str,
    messages_in: (),
    arg_fn: impl for<'a> Fn(&mut v8::HandleScope<'a>) -> Vec<v8::Local<'a, v8::Value>>,
) -> Result<(), AnyError> {
    let script_span = info_span!("js_run_script");
    let _guard = script_span.enter();
    // set up scene i/o
    let op_state = runtime.op_state();
    op_state.borrow_mut().put(messages_in);

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
    futures_lite::future::block_on(f).map(|_| ())
}

const MODULE_PREFIX: &str = "./assets/modules/";
const MODULE_SUFFIX: &str = ".js";
const SCENE_PREFIX: &str = "./assets/scenes/";

// synchronously returns a string containing JS code from the file system
#[op(v8)]
fn op_require(
    state: Rc<RefCell<OpState>>,
    module_spec: String,
) -> Result<String, deno_core::error::AnyError> {
    // only allow items within designated paths
    if module_spec.contains("..") {
        return Err(generic_error(format!(
            "invalid module request: '..' not allowed in `{module_spec}`"
        )));
    }

    let (scheme, name) = module_spec.split_at(1);
    let filename = match (scheme, name) {
        // core module load
        ("~", name) => format!("{MODULE_PREFIX}{name}{MODULE_SUFFIX}"),
        // generic load from the script path
        (scheme, name) => {
            let state = state.borrow();
            let path = &state.borrow::<SceneSceneContext>().definition.path;
            format!("{SCENE_PREFIX}{path}/{scheme}{name}")
        }
    };

    debug!("require(\"{filename}\")");

    std::fs::read_to_string(filename)
        .map_err(|err| generic_error(format!("invalid module request `{module_spec}` ({err})")))
}