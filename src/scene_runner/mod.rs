use std::{cell::RefCell, rc::Rc};

use bevy::{
    prelude::*,
    utils::{HashMap, HashSet},
};
use deno_core::{
    error::generic_error,
    include_js_files, op,
    v8::{self},
    Extension, JsRuntime, OpState, RuntimeOptions,
};
use serde::Serialize;

use crate::{crdt::CrdtComponentInterfaces, dcl_assert, dcl_component::SceneEntityId};

pub mod engine;

#[cfg(test)]
pub mod test;

// system sets used for ordering
#[derive(SystemSet, Debug, PartialEq, Eq, Hash, Clone)]
pub enum SceneSets {
    Init,          // setup the scene
    Input, // systems which create EngineResponses for the current frame (though these can be created anywhere)
    Run,   // run the script
    CreateDestroy, // manage entity lifetimes
    HandleOutput, // systems which handle events from the current frame
}

// (non-send) resource to hold the runtime and module object
#[derive(Default)]
pub struct JsRuntimeResource(HashMap<Entity, (JsRuntime, v8::Global<v8::Value>)>);

// metadata about the current scene. currently only the path (used in op_require to validate access)
#[derive(Clone, Default)]
pub struct SceneDefinition {
    pub path: String,
}

// event which can be sent from anywhere to trigger replacing the current scene with the one specified
pub struct LoadJsSceneEvent {
    pub scene: SceneDefinition,
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

// contains a list of (SceneEntityId.generation, bevy entity) indexed by SceneEntityId.id
// where generation is the earliest non-dead (though maybe not yet live)
// generation for the scene id index.
// entities are initialized within the engine message-loop op, and added to 'nascent' until
// the process_lifecycle system enlivens them.
// Bevy entities are only created on a PUT of a component we care about in the renderer,
// or if they are required for hierarchy parenting
// TODO - consider Vec<Option<page>>
type LiveEntityTable = Vec<(u16, Option<Entity>)>;

// mapping from script entity -> bevy entity
// note - be careful with size as this struct is moved into/out of js runtimes
#[derive(Component, Default)]
pub struct SceneContext {
    pub definition: SceneDefinition,

    // entities waiting to be born in bevy
    pub nascent: HashSet<SceneEntityId>,
    // entities waiting to be destroyed in bevy
    pub death_row: HashSet<(SceneEntityId, Entity)>,
    // entities that are live
    live_entities: LiveEntityTable,

    // list of entities that are not currently parented to their target parent
    pub unparented_entities: Vec<Entity>,
    // indicates if we need to reprocess unparented entities
    pub hierarchy_changed: bool,
}

impl SceneContext {
    pub fn new(definition: SceneDefinition, root: Entity) -> Self {
        let mut new_context = Self {
            definition,
            nascent: Default::default(),
            death_row: Default::default(),
            live_entities: Vec::from_iter(std::iter::repeat((0, None)).take(u16::MAX as usize)),
            unparented_entities: Vec::new(),
            hierarchy_changed: false,
        };

        new_context.live_entities[SceneEntityId::ROOT.id as usize] =
            (SceneEntityId::ROOT.generation, Some(root));
        new_context
    }

    fn entity_entry(&self, id: u16) -> &(u16, Option<Entity>) {
        // SAFETY: live entities has u16::MAX members
        unsafe { self.live_entities.get_unchecked(id as usize) }
    }

    fn entity_entry_mut(&mut self, id: u16) -> &mut (u16, Option<Entity>) {
        // SAFETY: live entities has u16::MAX members
        unsafe { self.live_entities.get_unchecked_mut(id as usize) }
    }

    // queue an entity for creation if required
    // returns false if the entity is already dead
    pub fn init(&mut self, entity: SceneEntityId) -> bool {
        debug!(" init {:?}!", entity);
        if self.is_dead(entity) {
            debug!("{:?} is dead!", entity);
            return false;
        }

        if !self.is_live_in_bevy(entity) {
            debug!("scene added {entity:?}");
            self.nascent.insert(entity);
        } else {
            debug!("{:?} is live already!", entity);
        }

        true
    }

    pub fn associate_bevy_entity(&mut self, scene_entity: SceneEntityId, bevy_entity: Entity) {
        debug!(
            "associate scene id: {} -> bevy id {:?}",
            scene_entity, bevy_entity
        );
        dcl_assert!(self.entity_entry(scene_entity.id).0 <= scene_entity.generation);
        dcl_assert!(self.entity_entry(scene_entity.id).1.is_none());
        *self.entity_entry_mut(scene_entity.id) = (scene_entity.generation, Some(bevy_entity));
    }

    pub fn kill(&mut self, scene_entity: SceneEntityId) {
        // update entity table and death row
        match self.entity_entry_mut(scene_entity.id) {
            (gen, maybe_bevy_entity) if *gen <= scene_entity.generation => {
                *gen = scene_entity.generation + 1;

                if let Some(bevy_entity) = maybe_bevy_entity.take() {
                    self.death_row.insert((scene_entity, bevy_entity));
                }
            }
            _ => (),
        }

        // remove from nascent
        self.nascent.remove(&scene_entity);
        debug!("scene killed {scene_entity:?}");
    }

    pub fn bevy_entity(&self, scene_entity: SceneEntityId) -> Option<Entity> {
        match self.entity_entry(scene_entity.id) {
            (gen, Some(bevy_entity)) if *gen == scene_entity.generation => Some(*bevy_entity),
            _ => None,
        }
    }

    pub fn is_live_in_bevy(&self, scene_entity: SceneEntityId) -> bool {
        self.nascent.contains(&scene_entity) || {
            let entry = self.entity_entry(scene_entity.id);
            entry.0 == scene_entity.generation && entry.1.is_some()
        }
    }

    pub fn is_dead(&self, entity: SceneEntityId) -> bool {
        self.entity_entry(entity.id).0 > entity.generation
    }
}

#[derive(Component)]
pub struct SceneEntity {
    pub root: Entity,
    pub scene_id: SceneEntityId,
}

// plugin which creates and runs scripts
pub struct SceneRunnerPlugin;

impl Plugin for SceneRunnerPlugin {
    fn build(&self, app: &mut App) {
        app.init_non_send_resource::<JsRuntimeResource>();
        app.init_resource::<CrdtComponentInterfaces>();

        app.add_event::<LoadJsSceneEvent>();
        app.add_event::<EngineResponse>();

        app.configure_sets(
            (
                SceneSets::Init,
                SceneSets::Input,
                SceneSets::Run,
                SceneSets::CreateDestroy,
                SceneSets::HandleOutput,
            )
                .chain(),
        );

        app.add_system(initialize_scene.in_set(SceneSets::Init));
        app.add_system(run_scene.in_set(SceneSets::Run));
        app.add_system(process_lifecycle.in_set(SceneSets::CreateDestroy));

        // add a command flush between CreateDestroy and HandleOutput so that
        // commands can be applied to entities in the same frame they are created
        app.add_system(
            apply_system_buffers
                .after(SceneSets::CreateDestroy)
                .before(SceneSets::HandleOutput),
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
            let path = &state.borrow::<SceneContext>().definition.path;
            format!("{SCENE_PREFIX}{path}/{scheme}{name}")
        }
    };

    debug!("require(\"{filename}\")");

    std::fs::read_to_string(filename)
        .map_err(|err| generic_error(format!("invalid module request `{module_spec}` ({err})")))
}

fn initialize_scene(
    mut runtime_res: NonSendMut<JsRuntimeResource>,
    mut load_scene_events: EventReader<LoadJsSceneEvent>,
    mut commands: Commands,
    crdt_component_interfaces: Res<CrdtComponentInterfaces>,
) {
    for new_scene in load_scene_events.iter() {
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

        // create the scene root entity
        // todo set world position
        let root = commands
            .spawn((SpatialBundle::default(), DeletedSceneEntities::default()))
            .id();
        commands.entity(root).insert(SceneEntity {
            root,
            scene_id: SceneEntityId::ROOT,
        });

        let context = SceneContext::new(new_scene.scene.clone(), root);

        let state = runtime.op_state();

        // store scene detail in the runtime state
        let mut state_mut = state.borrow_mut();
        state_mut.put(context);

        // store the component writers
        state_mut.put(crdt_component_interfaces.clone());

        drop(state_mut);

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
        run_script(
            &mut runtime,
            &script,
            "onStart",
            EngineResponseList::default(),
            |_| Vec::new(),
        )
        .unwrap();

        // process any engine commands
        let mut state_mut = state.borrow_mut();
        let mut root_commands = commands.entity(root);
        for crdt_interface in crdt_component_interfaces.0.values() {
            crdt_interface.claim_crdt(&mut state_mut, &mut root_commands);
        }

        // retrieve the entity_map
        let context = state_mut.take::<SceneContext>();

        // store entity map on the root entity
        commands.entity(root).insert(context);

        // insert runtime into the bevy app
        runtime_res.0.insert(root, (runtime, script));
    }
}

#[derive(Default)]
struct EngineResponseList(Vec<EngineResponse>);

// system to run the current active script
fn run_scene(
    mut commands: Commands,
    mut scenes: Query<(Entity, &mut SceneContext)>,
    mut runtime_res: NonSendMut<JsRuntimeResource>,
    mut engine_responses: EventReader<EngineResponse>,
    time: Res<Time>,
    crdt_interfaces: Res<CrdtComponentInterfaces>,
) {
    for (root, mut context_mut) in scenes.iter_mut() {
        if let Some((runtime, script)) = runtime_res.0.get_mut(&root) {
            let response_list = engine_responses.iter().cloned().collect();

            let context = std::mem::take(context_mut.as_mut());

            let op_state = runtime.op_state();
            op_state.borrow_mut().put(context);

            // run the onUpdate function
            run_script(
                runtime,
                script,
                "onUpdate",
                EngineResponseList(response_list),
                |scope| vec![v8::Number::new(scope, time.delta_seconds_f64()).into()],
            )
            .unwrap();

            // process any engine commands
            let state = runtime.op_state();
            let mut state_mut = state.borrow_mut();
            let mut root_commands = commands.entity(root);
            for crdt_interface in crdt_interfaces.0.values() {
                crdt_interface.claim_crdt(&mut state_mut, &mut root_commands);
            }

            // retrieve the entity map
            *context_mut = state_mut.take::<SceneContext>();
        } else {
            // discard events with no scene to receive them
            engine_responses.clear();
        }
    }
}

// helper to setup, acquire, run and return results from a script function
fn run_script(
    runtime: &mut JsRuntime,
    script: &v8::Global<v8::Value>,
    fn_name: &str,
    messages_in: EngineResponseList,
    arg_fn: impl for<'a> Fn(&mut v8::HandleScope<'a>) -> Vec<v8::Local<'a, v8::Value>>,
) -> Result<(), ()> {
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
            debug!("{fn_name} is not defined");
            return Err(());
        };
        let Ok(target_function) = v8::Local::<v8::Function>::try_from(target_function) else {
            error!("{fn_name} is not a function");
            return Err(());
        };

        // get args
        let args = arg_fn(scope);

        // call
        let res = target_function.call(scope, script_this, &args);

        let res = res.unwrap();

        drop(args);
        v8::Global::new(scope, res)
    };

    let f = runtime.resolve_value(promise);
    // TODO - all the multithreading ...
    futures_lite::future::block_on(f).unwrap();

    Ok(())
}

#[derive(Component, Default)]
pub struct DeletedSceneEntities(pub Vec<SceneEntityId>);

#[derive(Component)]
pub struct TargetParent(pub Entity);

fn process_lifecycle(
    mut commands: Commands,
    mut scenes: Query<(Entity, &mut SceneContext, &mut DeletedSceneEntities)>,
    children: Query<&Children>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (root_entity, mut context, mut deleted_entities) in scenes.iter_mut() {
        debug!("{:?}: nascent: {:?}", root_entity, context.nascent);
        commands.entity(root_entity).with_children(|root| {
            for create in std::mem::take(&mut context.nascent) {
                context.associate_bevy_entity(
                    create,
                    root.spawn((
                        PbrBundle {
                            // TODO remove these and replace with spatial bundle when mesh and material components are supported
                            mesh: meshes.add(shape::Cube::new(1.0).into()),
                            material: materials.add(Color::WHITE.into()),
                            ..Default::default()
                        },
                        SceneEntity {
                            root: root_entity,
                            scene_id: create,
                        },
                        TargetParent(root_entity),
                    ))
                    .id(),
                );

                debug!(
                    "spawned {:?} -> {:?}",
                    create,
                    context.bevy_entity(create).unwrap()
                );
            }
        });

        // update deleted entities list, used by crdt processors to filter results
        deleted_entities.0 = std::mem::take(&mut context.death_row)
            .into_iter()
            .map(|(deleted_scene_entity, deleted_bevy_entity)| {
                // reparent children to the root entity
                if let Ok(children) = children.get(deleted_bevy_entity) {
                    commands.entity(root_entity).push_children(children);
                }

                debug!(
                    "despawned {:?} -> {:?}",
                    deleted_scene_entity, deleted_bevy_entity
                );
                commands.entity(deleted_bevy_entity).despawn();
                deleted_scene_entity
            })
            .collect();
    }
}
