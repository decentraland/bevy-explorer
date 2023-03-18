use std::{
    cell::RefCell,
    rc::Rc,
    sync::mpsc::{sync_channel, Receiver, SyncSender, TryRecvError},
    time::{Duration, SystemTime},
};

use bevy::{
    prelude::*,
    utils::{FloatOrd, HashSet, Instant},
};
use deno_core::{
    error::{generic_error, AnyError},
    include_js_files, op,
    v8::{self, IsolateHandle},
    Extension, JsRuntime, OpState, RuntimeOptions,
};
use serde::Serialize;

use crate::{
    crdt::{CrdtComponentInterfaces, TypeMap},
    dcl_assert,
    dcl_component::SceneEntityId,
};

use self::engine::ShuttingDown;

pub mod engine;

#[cfg(test)]
pub mod test;

// system sets used for ordering
#[derive(SystemSet, Debug, PartialEq, Eq, Hash, Clone)]
pub enum SceneSets {
    Input, // systems which create EngineResponses for the current frame (though these can be created anywhere)
    Init,  // setup the scene
    RunLoop, // run the scripts
}

#[derive(SystemSet, Debug, PartialEq, Eq, Hash, Clone)]
pub enum SceneLoopSets {
    SendToScene,      // pass data to the scene
    ReceiveFromScene, // receive data from the scene
    Lifecycle,        // manage bevy entity lifetimes
    UpdateWorld,      // systems which handle events from the current frame
}

// data from renderer to scene
#[derive(Debug)]
pub enum RendererResponse {
    Ok(Vec<()>),
}

// data from scene to renderer
pub enum SceneResponse {
    Error(String),
    Ok(SceneCensus, TypeMap),
}

#[derive(Resource)]
pub struct SceneUpdates {
    pub sender: SyncSender<SceneResponse>,
    receiver: Receiver<SceneResponse>,
    pub jobs_in_flight: usize,
    pub update_deadline: SystemTime,
    pub eligible_jobs: usize,
}

// safety: struct is sync except for the receiver.
// receiver is only accessible via &mut handle
unsafe impl Sync for SceneUpdates {}

impl SceneUpdates {
    pub fn receiver(&mut self) -> &Receiver<SceneResponse> {
        &self.receiver
    }
}

#[derive(Component)]
pub struct SceneThreadHandle {
    pub kill_switch: IsolateHandle,
    pub sender: tokio::sync::mpsc::Sender<RendererResponse>,
}

// metadata about the current scene. currently only the path (used in op_require to validate access)
#[derive(Clone, Default, Debug)]
pub struct SceneDefinition {
    pub path: String,
    pub offset: Vec3,
    pub visible: bool,
}

// event which can be sent from anywhere to trigger replacing the current scene with the one specified
pub struct LoadSceneEvent {
    pub scene: SceneDefinition,
}

// fn kill_scenes(
//     mut commands: Commands,
//     scenes: Query<(Entity, &mut SceneThreadHandle)>,
//     time: Res<Time>,
// ) {
//     if time.elapsed_seconds() > 20.0 {
//         for (_ent, handle) in scenes.iter().skip(1) {
//             // let _ = futures_lite::future::block_on(
//             //     handle.sender.send(SceneThreadRendererResponses::Die),
//             // );
//             commands
//                 .entity(_ent)
//                 .remove::<RendererSceneContext>()
//                 .remove::<SceneThreadHandle>();
//             println!("-");
//             return;
//         }
//     }
// }

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
#[derive(Component, Default, Debug)]
pub struct RendererSceneContext {
    pub definition: SceneDefinition,
    pub priority: f32,

    // entities waiting to be born in bevy
    pub nascent: HashSet<SceneEntityId>,
    // entities waiting to be destroyed in bevy
    pub death_row: HashSet<SceneEntityId>,
    // entities that are live
    live_entities: LiveEntityTable,

    // list of entities that are not currently parented to their target parent
    pub unparented_entities: HashSet<Entity>,
    // indicates if we need to reprocess unparented entities
    pub hierarchy_changed: bool,

    // time of last message sent to scene
    pub last_sent: f32,
    pub in_flight: bool,
}

impl RendererSceneContext {
    pub fn new(definition: SceneDefinition, root: Entity, priority: f32) -> Self {
        let mut new_context = Self {
            definition,
            nascent: Default::default(),
            death_row: Default::default(),
            live_entities: Vec::from_iter(std::iter::repeat((0, None)).take(u16::MAX as usize)),
            unparented_entities: HashSet::new(),
            hierarchy_changed: false,
            last_sent: 0.0,
            in_flight: true,
            priority,
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

    pub fn associate_bevy_entity(&mut self, scene_entity: SceneEntityId, bevy_entity: Entity) {
        debug!(
            "associate scene id: {} -> bevy id {:?}",
            scene_entity, bevy_entity
        );
        dcl_assert!(self.entity_entry(scene_entity.id).0 <= scene_entity.generation);
        dcl_assert!(self.entity_entry(scene_entity.id).1.is_none());
        *self.entity_entry_mut(scene_entity.id) = (scene_entity.generation, Some(bevy_entity));
    }

    pub fn bevy_entity(&self, scene_entity: SceneEntityId) -> Option<Entity> {
        match self.entity_entry(scene_entity.id) {
            (gen, Some(bevy_entity)) if *gen == scene_entity.generation => Some(*bevy_entity),
            _ => None,
        }
    }

    pub fn is_dead(&self, entity: SceneEntityId) -> bool {
        self.entity_entry(entity.id).0 > entity.generation
    }
}

pub struct SceneCensus {
    root: Entity,
    born: HashSet<SceneEntityId>,
    died: HashSet<SceneEntityId>,
}

type LiveTable = Vec<(u16, bool)>;

pub struct SceneSceneContext {
    definition: SceneDefinition,
    root: Entity,
    live_entities: LiveTable,
    nascent: HashSet<SceneEntityId>,
    death_row: HashSet<SceneEntityId>,
}

impl SceneSceneContext {
    pub fn new(definition: SceneDefinition, root: Entity) -> Self {
        Self {
            definition,
            root,
            live_entities: Vec::from_iter(std::iter::repeat((0, false)).take(u16::MAX as usize)),
            nascent: Default::default(),
            death_row: Default::default(),
        }
    }

    fn entity_entry(&self, id: u16) -> &(u16, bool) {
        // SAFETY: live entities has u16::MAX members
        unsafe { self.live_entities.get_unchecked(id as usize) }
    }

    // queue an entity for creation if required
    // returns false if the entity is already dead
    pub fn init(&mut self, entity: SceneEntityId) -> bool {
        // debug!(" init {:?}!", entity);
        if self.is_dead(entity) {
            debug!("{:?} is dead!", entity);
            return false;
        }

        if !self.is_born(entity) {
            debug!("scene added {entity:?}");
            self.nascent.insert(entity);
        } else {
            // debug!("{:?} is live already!", entity);
        }

        true
    }

    pub fn take_census(&mut self) -> SceneCensus {
        for scene_entity in &self.nascent {
            self.live_entities[scene_entity.id as usize] = (scene_entity.generation, true);
        }

        SceneCensus {
            root: self.root,
            born: std::mem::take(&mut self.nascent),
            died: std::mem::take(&mut self.death_row),
        }
    }

    pub fn kill(&mut self, scene_entity: SceneEntityId) {
        // update entity table and death row
        match &mut self.live_entities[scene_entity.id as usize] {
            (gen, live) if *gen <= scene_entity.generation => {
                *gen = scene_entity.generation + 1;

                if *live {
                    self.death_row.insert(scene_entity);
                }
                *live = false;
            }
            _ => (),
        }

        // remove from nascent
        self.nascent.remove(&scene_entity);
        debug!("scene killed {scene_entity:?}");
    }

    pub fn is_born(&self, scene_entity: SceneEntityId) -> bool {
        self.nascent.contains(&scene_entity) || {
            let entry = self.entity_entry(scene_entity.id);
            entry.0 == scene_entity.generation && entry.1
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

#[derive(Resource)]
pub struct SceneLoopSchedule(pub Schedule);

impl Plugin for SceneRunnerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CrdtComponentInterfaces>();

        let (sender, receiver) = sync_channel(1000);
        app.insert_resource(SceneUpdates {
            sender,
            receiver,
            jobs_in_flight: 0,
            update_deadline: SystemTime::now(),
            eligible_jobs: 0,
        });

        app.add_event::<LoadSceneEvent>();
        app.add_event::<EngineResponse>();

        app.configure_sets((SceneSets::Input, SceneSets::Init, SceneSets::RunLoop).chain());
        app.add_system(
            apply_system_buffers
                .after(SceneSets::Init)
                .before(SceneSets::RunLoop),
        );
        app.add_system(initialize_scene.in_set(SceneSets::Init));
        app.add_system(update_scene_priority.in_set(SceneSets::Init));
        app.add_system(run_scene_loop.in_set(SceneSets::RunLoop));

        let mut scene_schedule = Schedule::new();

        scene_schedule.configure_sets(
            (
                SceneLoopSets::SendToScene,
                SceneLoopSets::ReceiveFromScene,
                SceneLoopSets::Lifecycle,
                SceneLoopSets::UpdateWorld,
            )
                .chain(),
        );

        scene_schedule.add_system(send_scene_updates.in_set(SceneLoopSets::SendToScene));
        scene_schedule.add_system(receive_scene_updates.in_set(SceneLoopSets::ReceiveFromScene));
        scene_schedule.add_system(process_lifecycle.in_set(SceneLoopSets::Lifecycle));

        // add a command flush between CreateDestroy and HandleOutput so that
        // commands can be applied to entities in the same frame they are created
        scene_schedule.add_system(
            apply_system_buffers
                .after(SceneLoopSets::Lifecycle)
                .before(SceneLoopSets::UpdateWorld),
        );

        app.insert_resource(SceneLoopSchedule(scene_schedule));
    }
}

fn run_scene_loop(world: &mut World) {
    world.resource_scope(|world, mut schedule: Mut<SceneLoopSchedule>| {
        // TODO: don't just use 5ms, determine a budget
        // - determine frame render time (bevy pr required - but maybe not needed with dynamic res scaling..?)
        // - determine main world frame time (can roughly do this by snapping around render world extract, but better with bevy pr)
        // - use prior frame scene budget and main world frame time to determine main world time excluding scenes
        // - use max of target frame time and render time to allocate scene budget
        // - trim for possibility of more work coming in
        let update_deadline = Duration::from_millis(5);
        let start = Instant::now();

        // always run once
        schedule.0.run(world);
        // run until time elapsed or all scenes are updated
        while Instant::now().duration_since(start) < update_deadline
            && world.resource::<SceneUpdates>().eligible_jobs > 0
        {
            schedule.0.run(world)
        }
    });
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

fn update_scene_priority(mut q: Query<&mut RendererSceneContext>) {
    for mut context in q.iter_mut() {
        context.priority = context.definition.offset.length().powf(1.0);
    }
}

fn initialize_scene(
    mut load_scene_events: EventReader<LoadSceneEvent>,
    mut commands: Commands,
    mut scene_updates: ResMut<SceneUpdates>,
    crdt_component_interfaces: Res<CrdtComponentInterfaces>,
    mut counter: Local<usize>,
) {
    for new_scene in load_scene_events.iter() {
        // create the scene root entity
        // todo set world position
        let root = commands
            .spawn((
                SpatialBundle {
                    transform: Transform::from_translation(new_scene.scene.offset),
                    visibility: if new_scene.scene.visible {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    },
                    ..Default::default()
                },
                DeletedSceneEntities::default(),
            ))
            .id();
        commands.entity(root).insert(SceneEntity {
            root,
            scene_id: SceneEntityId::ROOT,
        });

        let scene_context = SceneSceneContext::new(new_scene.scene.clone(), root);
        let renderer_context = RendererSceneContext::new(new_scene.scene.clone(), root, 1.0);

        let (main_sx, thread_rx) = tokio::sync::mpsc::channel::<RendererResponse>(1);
        let (handle_sx, handle_rx) = sync_channel::<IsolateHandle>(1);
        let thread_sx = scene_updates.sender.clone();

        let crdt_component_interfaces = crdt_component_interfaces.clone();

        std::thread::Builder::new()
            .name(format!("scene thread {}", *counter))
            .spawn(move || {
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

                // send handle to main thread
                handle_sx
                    .send(runtime.v8_isolate().thread_safe_handle())
                    .unwrap_or_else(|e| error!("handle channel closed: {e:?}"));

                let state = runtime.op_state();

                // store scene detail in the runtime state
                state.borrow_mut().put(scene_context);

                // store the component writers
                state.borrow_mut().put(crdt_component_interfaces.clone());

                // store channels
                state.borrow_mut().put(thread_sx);
                state.borrow_mut().put(thread_rx);

                // store crdt state
                state.borrow_mut().put(TypeMap::default());

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
                let result = run_script(
                    &mut runtime,
                    &script,
                    "onStart",
                    EngineResponseList::default(),
                    |_| Vec::new(),
                );

                if let Err(e) = result {
                    // ignore failure to send failure
                    let _ = state
                        .borrow_mut()
                        .take::<SyncSender<SceneResponse>>()
                        .send(SceneResponse::Error(format!("{e:?}")));
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
                    let result = run_script(
                        &mut runtime,
                        &script,
                        "onUpdate",
                        EngineResponseList::default(),
                        |scope| vec![v8::Number::new(scope, dt.as_secs_f64()).into()],
                    );

                    if state.borrow().try_borrow::<ShuttingDown>().is_some() {
                        return;
                    }

                    if let Err(e) = result {
                        let _ = state
                            .borrow_mut()
                            .take::<SyncSender<SceneResponse>>()
                            .send(SceneResponse::Error(format!("{e:?}")));
                        return;
                    }
                }
            })
            .unwrap();
        *counter += 1;

        match handle_rx.recv_timeout(Duration::from_secs(1)) {
            Ok(kill_switch) => {
                // store entity map on the root entity
                commands.entity(root).insert((
                    renderer_context,
                    SceneThreadHandle {
                        kill_switch,
                        sender: main_sx,
                    },
                ));
                scene_updates.jobs_in_flight += 1;
            }
            Err(e) => {
                error!("failed to spawn scene thread: {e:?}");
            }
        }
    }
}

#[derive(Default)]
struct EngineResponseList(Vec<EngineResponse>);

const MAX_CONCURRENT_SCENES: usize = 16;

fn send_scene_updates(
    mut scenes: Query<(Entity, &mut RendererSceneContext, &SceneThreadHandle)>,
    mut updates: ResMut<SceneUpdates>,
    time: Res<Time>,
) {
    updates.eligible_jobs = 0;

    // sort eligible scenes
    let mut sorted_scenes: Vec<_> = scenes
        .iter()
        .filter(|(_, context, _)| !context.in_flight)
        .filter_map(|(ent, context, _)| {
            let not_yet_run = context.last_sent < time.elapsed_seconds();
            if not_yet_run {
                updates.eligible_jobs += 1;
            }

            (!context.in_flight && not_yet_run).then(|| {
                let priority =
                    FloatOrd(context.priority / (time.elapsed_seconds() - context.last_sent));
                (ent, priority)
            })
        })
        .collect();
    sorted_scenes.sort_by_key(|(_, priority)| *priority);

    for (ent, _) in sorted_scenes
        .into_iter()
        .take(MAX_CONCURRENT_SCENES.saturating_sub(updates.jobs_in_flight))
    {
        let (_, mut context, handle) = scenes.get_mut(ent).unwrap();
        if let Err(e) = handle
            .sender
            .blocking_send(RendererResponse::Ok(Vec::default()))
        {
            error!("failed to send updates to scene: {e:?}");
            // TODO: clean up
        } else {
            context.last_sent = time.elapsed_seconds();
            context.in_flight = true;
            updates.jobs_in_flight += 1;
        }
    }
}

// system to run the current active script
fn receive_scene_updates(
    mut commands: Commands,
    mut updates: ResMut<SceneUpdates>,
    mut scenes: Query<&mut RendererSceneContext>,
    crdt_interfaces: Res<CrdtComponentInterfaces>,
) {
    loop {
        match updates.receiver().try_recv() {
            Ok(response) => {
                match response {
                    SceneResponse::Error(msg) => {
                        error!("scene error: {msg}");
                        // *context_mut = context;
                    }
                    SceneResponse::Ok(census, mut crdt) => {
                        debug!(
                            "scene {:?} received updates! [+{}, -{}]",
                            census.root,
                            census.born.len(),
                            census.died.len()
                        );
                        if let Ok(mut context) = scenes.get_mut(census.root) {
                            context.in_flight = false;
                            context.nascent = census.born;
                            context.death_row = census.died;
                            let mut commands = commands.entity(census.root);
                            for interface in crdt_interfaces.0.values() {
                                interface.updates_to_entity(&mut crdt, &mut commands);
                            }
                        } else {
                            debug!("no scene entity, probably got dropped before we processed the result");
                        }
                    }
                }
                updates.jobs_in_flight -= 1;
            }
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                panic!("render thread receiver exploded");
            }
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

#[derive(Component, Default)]
pub struct DeletedSceneEntities(pub HashSet<SceneEntityId>);

#[derive(Component)]
pub struct TargetParent(pub Entity);

fn process_lifecycle(
    mut commands: Commands,
    mut scenes: Query<(Entity, &mut RendererSceneContext, &mut DeletedSceneEntities)>,
    children: Query<&Children>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut handles: Local<Option<(Handle<Mesh>, Handle<StandardMaterial>)>>,
) {
    let (mesh, material) = handles.get_or_insert_with(|| {
        (
            meshes.add(shape::Cube::new(1.0).into()),
            materials.add(Color::WHITE.into()),
        )
    });

    for (root_entity, mut context, mut deleted_entities) in scenes.iter_mut() {
        if !context.nascent.is_empty() {
            debug!("{:?}: nascent: {:?}", root_entity, context.nascent);
        }
        commands.entity(root_entity).with_children(|root| {
            for create in std::mem::take(&mut context.nascent) {
                if context.bevy_entity(create).is_some() {
                    continue;
                }
                context.associate_bevy_entity(
                    create,
                    root.spawn((
                        PbrBundle {
                            // TODO remove these and replace with spatial bundle when mesh and material components are supported
                            mesh: mesh.clone(),
                            material: material.clone(),
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
        deleted_entities.0 = std::mem::take(&mut context.death_row);

        for deleted_scene_entity in &deleted_entities.0 {
            if let Some(deleted_bevy_entity) = context.bevy_entity(*deleted_scene_entity) {
                // reparent children to the root entity
                if let Ok(children) = children.get(deleted_bevy_entity) {
                    commands.entity(root_entity).push_children(children);
                }

                debug!(
                    "despawned {:?} -> {:?}",
                    deleted_scene_entity, deleted_bevy_entity
                );
                commands.entity(deleted_bevy_entity).despawn();
            }
        }
    }
}
