use std::{cell::RefCell, rc::Rc};

use bevy::{ecs::event::Event, prelude::*, utils::HashMap};
use deno_core::{
    error::generic_error, include_js_files, op, v8, Extension, JsRuntime, OpState, RuntimeOptions,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::value::RawValue;

mod engine;

// system sets used for ordering
#[derive(SystemLabel)]
pub enum SceneSystems {
    Init,         // setup the scene
    Input, // systems which create EngineResponses for the current frame (though these can be created anywhere)
    Run,   // run the script
    SpawnEvents, // generate bevy events from EngineCommands
    HandleOutput, // systems which handle events from the current frame
}

// resource to hold the runtime and module object
#[derive(Default)]
pub struct JsRuntimeResource(Option<(JsRuntime, v8::Global<v8::Value>)>);

// metadata about the current scene. currently only the path (used in op_require to validate access)
#[derive(Clone)]
pub struct JsScene {
    pub path: String,
}

// event which can be sent from anywhere to trigger replacing the current scene with the one specified
pub struct LoadJsSceneEvent {
    pub scene: JsScene,
}

// struct used for sending responses to the script.
#[derive(Clone, Serialize)]
pub struct EngineResponse {
    pub method: String,
    pub data: serde_json::Value,
}

impl EngineResponse {
    // create from a method name and any type which implements `Serialize`
    pub fn new(method: String, data: impl Serialize) -> Self {
        Self {
            method,
            data: serde_json::to_value(data).unwrap(),
        }
    }
}

// struct used for receiving commands from the script.
#[derive(Clone, Debug, Deserialize)]
pub struct EngineCommand {
    pub method: String,
    // it's a bit unfortunate that we need a boxed copy of the payload here. we could
    // work around it but for a proof of concept it'll do..
    pub data: Box<RawValue>,
}

// mapping from script entity -> bevy entity
#[derive(Resource, Default)]
pub struct JsEntityMap(pub HashMap<usize, Entity>);

// plugin which creates and runs scripts
pub struct SceneRunnerPlugin;

impl Plugin for SceneRunnerPlugin {
    fn build(&self, app: &mut App) {
        app.init_non_send_resource::<JsRuntimeResource>();
        app.init_resource::<JsEntityMap>();

        app.add_event::<LoadJsSceneEvent>();
        app.add_event::<EngineResponse>();
        app.add_event::<EngineCommand>();

        app.add_system(
            initialize_scene
                .label(SceneSystems::Init)
                .before(SceneSystems::Input),
        );
        app.add_system(
            run_scene
                .after(SceneSystems::Input)
                .label(SceneSystems::Run)
                .before(SceneSystems::SpawnEvents)
                .before(SceneSystems::HandleOutput),
        );
    }
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
            "invalid module request `{module_spec}`"
        )));
    }

    let (scheme, name) = module_spec.split_at(1);
    let filename = match (scheme, name) {
        // core module load
        ("~", name) => format!("{MODULE_PREFIX}{name}{MODULE_SUFFIX}"),
        // generic load from the script path
        (scheme, name) => {
            let state = state.borrow();
            let path = &state.borrow::<JsScene>().path;
            format!("{SCENE_PREFIX}{path}/{scheme}{name}")
        }
    };

    info!("require(\"{filename}\")");

    std::fs::read_to_string(filename)
        .map_err(|_| generic_error(format!("invalid module request `{module_spec}`")))
}

fn initialize_scene(
    mut runtime_res: NonSendMut<JsRuntimeResource>,
    mut load_scene_events: EventReader<LoadJsSceneEvent>,
    mut commands: Commands,
    mut entities: ResMut<JsEntityMap>,
    mut engine_command_events: EventWriter<EngineCommand>,
) {
    let Some(new_scene) = load_scene_events.iter().last() else { return };

    // remove existing scene
    runtime_res.0 = None;
    // clear prev scene entities
    for (_, entity) in entities.0.drain() {
        commands.entity(entity).despawn_recursive();
    }

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
                "op_engine_send_message",
            ];
            if ALLOW.contains(&op.name) {
                op
            } else {
                op.disable()
            }
        })
        .build();

    // create runtime
    let mut runtime = JsRuntime::new(RuntimeOptions {
        extensions_with_js: vec![ext],
        ..Default::default()
    });

    // TODO: snapshot

    // store scene detail in the runtime state
    let state = runtime.op_state();
    state.borrow_mut().put(new_scene.scene.clone());

    // load module
    let script = runtime.execute_script("<loader>", "require (\"scene.js\")");

    let script = match script {
        Err(e) => {
            error!("script load error: {}", e);
            return;
        }
        Ok(script) => script,
    };

    // run startup function
    let engine_commands = run_script(
        &mut runtime,
        &script,
        "onStart",
        EngineResponseList::default(),
        |_| Vec::new(),
    );

    // process any engine commands
    if let Some(commands) = engine_commands {
        for command in commands.0 {
            engine_command_events.send(command);
        }
    }

    // insert runtime into the bevy app
    runtime_res.0 = Some((runtime, script));
}

#[derive(Default)]
struct EngineResponseList(Vec<EngineResponse>);
#[derive(Default)]
struct EngineCommandList(Vec<EngineCommand>);

// system to run the current active script
fn run_scene(
    mut runtime_res: NonSendMut<JsRuntimeResource>,
    mut engine_responses: EventReader<EngineResponse>,
    mut engine_command_events: EventWriter<EngineCommand>,
    time: Res<Time>,
) {
    if let Some((runtime, script)) = &mut runtime_res.0 {
        let response_list = engine_responses.iter().cloned().collect();

        // run the onUpdate function
        let engine_commands = run_script(
            runtime,
            script,
            "onUpdate",
            EngineResponseList(response_list),
            |scope| vec![v8::Number::new(scope, time.delta_seconds_f64()).into()],
        );

        // process any engine commands
        if let Some(engine_commands) = engine_commands {
            for engine_command in engine_commands.0 {
                engine_command_events.send(engine_command);
            }
        }
    } else {
        // discard events with no scene to receive them
        engine_responses.clear();
    }
}

// helper to setup, acquire, run and return results from a script function
fn run_script<'s>(
    runtime: &'s mut JsRuntime,
    script: &v8::Global<v8::Value>,
    fn_name: &str,
    messages_in: EngineResponseList,
    arg_fn: impl Fn(&mut v8::HandleScope<'s>) -> Vec<v8::Local<'s, v8::Value>>,
) -> Option<EngineCommandList> {
    // set up scene i/o
    let op_state = runtime.op_state();
    op_state.borrow_mut().put(messages_in);
    op_state.borrow_mut().put(EngineCommandList::default());

    // get module
    let scope = &mut runtime.handle_scope();
    let script_value = v8::Local::new(scope, script.clone());
    let script = v8::Local::<v8::Object>::try_from(script_value).unwrap();

    // get function
    let on_update =
        v8::String::new_from_utf8(scope, fn_name.as_bytes(), v8::NewStringType::Internalized)
            .unwrap();
    let Some(on_update) = script.get(scope, on_update.into()) else {
        // function not define, is that an error ?
        info!("{fn_name} is not defined");
        return None;
    };
    let Ok(on_update) = v8::Local::<v8::Function>::try_from(on_update) else {
        error!("{fn_name} is not a function");
        return None;
    };

    // get args
    let args = arg_fn(scope);

    // call
    on_update.call(scope, script_value, &args);

    // gather and return results
    let commands = op_state.borrow_mut().take::<EngineCommandList>();
    Some(commands)
}

// a helper to automatically multiplex engine command json and generate bevy events
pub trait AddEngineCommandHandlerExt {
    fn add_command_event<T: Event + DeserializeOwned>(&mut self, method: &'static str);
}

impl AddEngineCommandHandlerExt for App {
    fn add_command_event<E: Event + DeserializeOwned>(&mut self, method: &'static str) {
        // register the target event
        self.add_event::<E>();

        // TODO: store the method name in a resource so we can warn on unrecognised commands

        // system (as a closure) that checks each engine command to see if the method matches,
        // and then creates and posts the target bevy event
        let system = move |mut reader: EventReader<EngineCommand>, mut writer: EventWriter<E>| {
            for engine_command in reader.iter() {
                if engine_command.method == *method {
                    let event: Result<E, _> = serde_json::from_str(engine_command.data.get());
                    match event {
                        Ok(event) => writer.send(event),
                        Err(e) => error!("malformed payload for {:?}: {}", engine_command, e),
                    }
                }
            }
        };

        self.add_system(
            system
                .label(SceneSystems::SpawnEvents)
                .before(SceneSystems::HandleOutput),
        );
    }
}
