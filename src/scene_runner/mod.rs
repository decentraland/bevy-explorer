use std::{
    collections::VecDeque,
    sync::mpsc::{sync_channel, Receiver, SyncSender, TryRecvError},
    time::{Duration, SystemTime},
};

use bevy::{
    prelude::*,
    utils::{FloatOrd, HashMap, HashSet, Instant},
};

use crate::{
    dcl::{
        interface::{CrdtComponentInterfaces, CrdtStore, CrdtType},
        spawn_scene, RendererResponse, SceneId, SceneResponse,
    },
    dcl_assert,
    dcl_component::{
        transform_and_parent::DclTransformAndParent, DclReader, DclWriter, SceneComponentId,
        SceneEntityId, ToDclWriter,
    },
    ipfs::{IpfsLoaderExt, SceneDefinition, SceneIpfsLocation, SceneJsFile, SceneMeta},
};

use self::update_world::{CrdtExtractors, SceneOutputPlugin};

#[cfg(test)]
pub mod test;
pub mod update_world;

// system sets used for ordering
#[derive(SystemSet, Debug, PartialEq, Eq, Hash, Clone)]
pub enum SceneSets {
    Init,     // setup the scene
    PostInit, // used for adding data to new scenes
    Input, // systems which create EngineResponses for the current frame (though these can be created anywhere)
    RunLoop, // run the scripts
    PostLoop, // do anything after the script loop
}

#[derive(SystemSet, Debug, PartialEq, Eq, Hash, Clone)]
pub enum SceneLoopSets {
    SendToScene,      // pass data to the scene
    ReceiveFromScene, // receive data from the scene
    Lifecycle,        // manage bevy entity lifetimes
    UpdateWorld,      // systems which handle events from the current frame
}

#[derive(Resource)]
pub struct SceneUpdates {
    pub sender: SyncSender<SceneResponse>,
    receiver: Receiver<SceneResponse>,
    pub scene_ids: HashMap<SceneId, Entity>,
    pub jobs_in_flight: usize,
    pub update_deadline: SystemTime,
    pub eligible_jobs: usize,
    pub loop_end_time: Instant,
    pub scene_queue: VecDeque<(Entity, FloatOrd)>,
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
    pub sender: tokio::sync::mpsc::Sender<RendererResponse>,
}

// event which can be sent from anywhere to trigger replacing the current scene with the one specified
pub struct LoadSceneEvent {
    pub location: SceneIpfsLocation,
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
#[derive(Component, Debug)]
pub struct RendererSceneContext {
    pub scene_id: SceneId,
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
    // currently running?
    pub in_flight: bool,

    pub crdt_store: CrdtStore,
}

impl RendererSceneContext {
    pub fn new(scene_id: SceneId, root: Entity, priority: f32) -> Self {
        let mut new_context = Self {
            scene_id,
            nascent: Default::default(),
            death_row: Default::default(),
            live_entities: Vec::from_iter(std::iter::repeat((0, None)).take(u16::MAX as usize)),
            unparented_entities: HashSet::new(),
            hierarchy_changed: false,
            last_sent: 0.0,
            in_flight: false,
            priority,
            crdt_store: Default::default(),
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

    pub fn update_crdt(
        &mut self,
        component_id: SceneComponentId,
        crdt_type: CrdtType,
        id: SceneEntityId,
        data: &impl ToDclWriter,
    ) {
        let mut buf = Vec::new();
        DclWriter::new(&mut buf).write(data);
        self.crdt_store
            .force_update(component_id, crdt_type, id, Some(&mut DclReader::new(&buf)))
    }

    #[allow(dead_code)]
    pub fn clear_crdt(
        &mut self,
        component_id: SceneComponentId,
        crdt_type: CrdtType,
        id: SceneEntityId,
    ) {
        self.crdt_store
            .force_update(component_id, crdt_type, id, None)
    }
}

#[derive(Component, Debug)]
pub struct SceneEntity {
    pub root: Entity,
    pub scene_id: SceneId,
    pub id: SceneEntityId,
}

// plugin which creates and runs scripts
pub struct SceneRunnerPlugin;

#[derive(Resource)]
pub struct SceneLoopSchedule {
    schedule: Schedule,
    end_time: Instant,
}

impl Plugin for SceneRunnerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CrdtExtractors>();

        let (sender, receiver) = sync_channel(1000);
        app.insert_resource(SceneUpdates {
            sender,
            receiver,
            scene_ids: Default::default(),
            jobs_in_flight: 0,
            update_deadline: SystemTime::now(),
            eligible_jobs: 0,
            scene_queue: Default::default(),
            loop_end_time: Instant::now(),
        });

        app.add_event::<LoadSceneEvent>();

        app.configure_sets(
            (
                SceneSets::Init,
                SceneSets::PostInit,
                SceneSets::Input,
                SceneSets::RunLoop,
                SceneSets::PostLoop,
            )
                .chain(),
        );
        app.add_system(
            apply_system_buffers
                .after(SceneSets::Init)
                .before(SceneSets::PostInit),
        );
        app.add_system(
            apply_system_buffers
                .after(SceneSets::PostInit)
                .before(SceneSets::RunLoop),
        );

        app.add_system(load_scene_entity.in_set(SceneSets::Init));
        app.add_system(load_scene_json.in_set(SceneSets::Init));
        app.add_system(load_scene_js.in_set(SceneSets::Init));
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

        app.insert_resource(SceneLoopSchedule {
            schedule: scene_schedule,
            end_time: Instant::now(),
        });

        app.add_plugin(SceneOutputPlugin);
    }
}

fn run_scene_loop(world: &mut World) {
    let mut loop_schedule = world.resource_mut::<SceneLoopSchedule>();
    let mut schedule = std::mem::take(&mut loop_schedule.schedule);
    let last_end_time = loop_schedule.end_time;
    let _start_time = Instant::now();
    #[cfg(debug_assertions)]
    let millis = 100;
    #[cfg(not(debug_assertions))]
    let millis = 6;
    let end_time = last_end_time + Duration::from_millis(millis);
    world.resource_mut::<SceneUpdates>().loop_end_time = end_time;

    // run at least once to collect updates even if no scenes are eligible
    let mut run_once = false;

    // run until time elapsed or all scenes are updated
    while Instant::now() < end_time
        && (!run_once
            || world.resource::<SceneUpdates>().eligible_jobs > 0
            || world.resource::<SceneUpdates>().jobs_in_flight > 0)
    {
        schedule.run(world);
        run_once = true;
    }

    // if !run_once {
    //     warn!("skip");
    // } else {
    //     info!(
    //         "frame: {}, loop: {}",
    //         (Instant::now().duration_since(last_end_time).as_secs_f64() * 1000.0) as u32,
    //         (Instant::now().duration_since(start_time).as_secs_f64() * 1000.0) as u32,
    //     );
    // }

    let mut loop_schedule = world.resource_mut::<SceneLoopSchedule>();
    loop_schedule.schedule = schedule;
    loop_schedule.end_time = Instant::now();
}

fn update_scene_priority(
    mut scenes: Query<(Entity, &GlobalTransform, &mut RendererSceneContext)>,
    camera: Query<&GlobalTransform, With<PrimaryCamera>>,
    mut updates: ResMut<SceneUpdates>,
    time: Res<Time>,
) {
    updates.eligible_jobs = 0;

    let camera_translation = camera
        .get_single()
        .map(|gt| gt.translation())
        .unwrap_or_default();

    // sort eligible scenes
    updates.scene_queue = scenes
        .iter_mut()
        .filter(|(_, _, context)| !context.in_flight)
        .filter_map(|(ent, transform, mut context)| {
            let distance = (transform.translation() - camera_translation).length();
            context.priority = distance;
            let not_yet_run = context.last_sent < time.elapsed_seconds();

            (!context.in_flight && not_yet_run).then(|| {
                updates.eligible_jobs += 1;
                let priority =
                    FloatOrd(context.priority / (time.elapsed_seconds() - context.last_sent));
                (ent, priority)
            })
        })
        .collect();
    updates
        .scene_queue
        .make_contiguous()
        .sort_by_key(|(_, priority)| *priority);
}

#[derive(Component)]
pub enum SceneLoading {
    SceneEntity,
    SceneMeta,
    Javascript,
}

fn load_scene_entity(
    mut commands: Commands,
    mut load_scene_events: EventReader<LoadSceneEvent>,
    asset_server: Res<AssetServer>,
) {
    for event in load_scene_events.iter() {
        match &event.location {
            SceneIpfsLocation::Pointer(x, y) => {
                commands.spawn((
                    SceneLoading::SceneEntity,
                    asset_server.load_scene_pointer(*x, *y),
                ));
            }
            SceneIpfsLocation::Hash(path) => {
                commands.spawn((
                    SceneLoading::SceneEntity,
                    asset_server.load::<SceneDefinition, _>(format!("{path}.scene_entity")),
                ));
            }
            SceneIpfsLocation::Js(path) => {
                commands.spawn((
                    SceneLoading::Javascript,
                    asset_server.load::<SceneJsFile, _>(format!("{path}.js")),
                ));
            }
        };
    }
}

fn load_scene_json(
    mut commands: Commands,
    mut loading_scenes: Query<(Entity, &mut SceneLoading, &Handle<SceneDefinition>)>,
    scene_definitions: Res<Assets<SceneDefinition>>,
    asset_server: Res<AssetServer>,
) {
    for (entity, mut state, h_scene) in loading_scenes
        .iter_mut()
        .filter(|(_, state, _)| matches!(**state, SceneLoading::SceneEntity))
    {
        let mut fail = |msg: &str| {
            warn!("{entity:?} failed to initialize scene: {msg}");
            commands.entity(entity).despawn_recursive();
        };

        match asset_server.get_load_state(h_scene) {
            bevy::asset::LoadState::Loaded => (),
            bevy::asset::LoadState::Failed => {
                fail("Scene entity could not be loaded");
                continue;
            }
            _ => continue,
        }
        let Some(definition) = scene_definitions.get(h_scene) else {
            fail("Scene entity did not resolve to a valid asset");
            continue;
        };
        let Some(h_meta) = asset_server.load_scene_file::<SceneMeta>("scene.json", &definition.content) else {
            fail("scene entity did not contain a `scene.json` content item");
            continue;
        };

        commands.entity(entity).insert(h_meta);
        *state = SceneLoading::SceneMeta;
    }
}

fn load_scene_js(
    mut commands: Commands,
    mut loading_scenes: Query<(
        Entity,
        &mut SceneLoading,
        &Handle<SceneDefinition>,
        &Handle<SceneMeta>,
    )>,
    scene_definitions: Res<Assets<SceneDefinition>>,
    scene_metas: Res<Assets<SceneMeta>>,
    asset_server: Res<AssetServer>,
) {
    for (entity, mut state, h_scene, h_meta) in loading_scenes
        .iter_mut()
        .filter(|(_, state, _, _)| matches!(**state, SceneLoading::SceneMeta))
    {
        let mut fail = |msg: &str| {
            warn!("{entity:?} failed to initialize scene: {msg}");
            commands.entity(entity).despawn_recursive();
        };

        match asset_server.get_load_state(h_meta) {
            bevy::asset::LoadState::Loaded => (),
            bevy::asset::LoadState::Failed => {
                fail("scene.json could not be loaded");
                continue;
            }
            _ => continue,
        }
        let definition = scene_definitions.get(h_scene).unwrap();
        let Some(meta) = scene_metas.get(h_meta) else {
            fail("scene.json did not resolve to expected format");
            continue;
        };
        let Some(h_code) = asset_server.load_scene_file::<SceneJsFile>(&meta.main, &definition.content) else {
            fail(format!("scene entity did not contain `main` content item `{}`", meta.main).as_str());
            continue;
        };

        commands.entity(entity).insert(h_code);
        *state = SceneLoading::Javascript;
    }
}

fn initialize_scene(
    mut commands: Commands,
    mut scene_updates: ResMut<SceneUpdates>,
    crdt_component_interfaces: Res<CrdtExtractors>,
    loading_scenes: Query<(Entity, &SceneLoading, &Handle<SceneJsFile>)>,
    scene_js_files: Res<Assets<SceneJsFile>>,
    asset_server: Res<AssetServer>,
) {
    for (root, _, h_code) in loading_scenes
        .iter()
        .filter(|(_, state, ..)| matches!(state, SceneLoading::Javascript))
    {
        debug!("checking for js");
        let mut fail = |msg: &str| {
            warn!("{root:?} failed to initialize scene: {msg}");
            commands.entity(root).despawn_recursive();
        };

        match asset_server.get_load_state(h_code) {
            bevy::asset::LoadState::Loaded => (),
            bevy::asset::LoadState::Failed => {
                fail("main js could not be loaded");
                continue;
            }
            _ => continue,
        }

        let Some(js_file) = scene_js_files.get(h_code) else {
            fail("main js did not resolve to expected format");
            continue;
        };

        info!("{root:?}: starting scene");

        // create the scene root entity
        // todo set world position
        commands.entity(root).remove::<SceneLoading>().insert((
            SpatialBundle {
                // todo set world position
                ..Default::default()
            },
            DeletedSceneEntities::default(),
        ));

        let thread_sx = scene_updates.sender.clone();

        let crdt_component_interfaces = CrdtComponentInterfaces(HashMap::from_iter(
            crdt_component_interfaces
                .0
                .iter()
                .map(|(id, interface)| (*id, interface.crdt_type())),
        ));

        let (scene_id, main_sx) =
            spawn_scene(js_file.clone(), crdt_component_interfaces, thread_sx);

        let renderer_context = RendererSceneContext::new(scene_id, root, 1.0);

        scene_updates.scene_ids.insert(scene_id, root);

        commands.entity(root).insert((
            renderer_context,
            SceneEntity {
                root,
                scene_id,
                id: SceneEntityId::ROOT,
            },
            SceneThreadHandle { sender: main_sx },
        ));
    }
}

// TODO: work out how to set this intelligently
// we need to keep enough scheduler time to ensure the main loop wakes enough
// otherwise we end up overrunning the budget
// also consider
// - reduce bevy async thread pool
// - reduce bevy primary thread pool
// - see if we can get v8 single threaded / no native threads working
const MAX_CONCURRENT_SCENES: usize = 8;

#[derive(Component)]
pub struct PrimaryCamera;

fn send_scene_updates(
    mut scenes: Query<(
        Entity,
        &mut RendererSceneContext,
        &SceneThreadHandle,
        &GlobalTransform,
    )>,
    mut updates: ResMut<SceneUpdates>,
    time: Res<Time>,
    camera: Query<&GlobalTransform, With<PrimaryCamera>>,
) {
    let updates = &mut *updates;

    if updates.jobs_in_flight == MAX_CONCURRENT_SCENES {
        return;
    }

    let Some((ent, _)) = updates.scene_queue.pop_front() else {
        return;
    };

    let (_, mut context, handle, scene_transform) = scenes.get_mut(ent).unwrap();

    // collect components

    // generate updates for camera and player
    let crdt_store = &mut context.crdt_store;
    let mut affine = camera.single().affine();
    affine.translation -= scene_transform.affine().translation;
    let camera_relative_transform = Transform::from(GlobalTransform::from(affine));
    let mut buf = Vec::default();
    let mut writer = DclWriter::new(&mut buf);
    writer.write(&DclTransformAndParent::from_bevy_transform_and_parent(
        &camera_relative_transform,
        SceneEntityId::ROOT,
    ));

    crdt_store.force_update(
        SceneComponentId::TRANSFORM,
        CrdtType::LWW_ENT,
        SceneEntityId::CAMERA,
        Some(&mut writer.reader()),
    );

    crdt_store.force_update(
        SceneComponentId::TRANSFORM,
        CrdtType::LWW_ENT,
        SceneEntityId::PLAYER,
        Some(&mut writer.reader()),
    );

    if let Err(e) = handle
        .sender
        .blocking_send(RendererResponse::Ok(crdt_store.take_updates()))
    {
        error!("failed to send updates to scene: {e:?}");
        // TODO: clean up
    } else {
        context.last_sent = time.elapsed_seconds();
        context.in_flight = true;
        updates.jobs_in_flight += 1;
    }

    updates.eligible_jobs -= 1;
}

// system to run the current active script
fn receive_scene_updates(
    mut commands: Commands,
    mut updates: ResMut<SceneUpdates>,
    mut scenes: Query<&mut RendererSceneContext>,
    crdt_interfaces: Res<CrdtExtractors>,
) {
    loop {
        match updates.receiver().try_recv() {
            Ok(response) => {
                match response {
                    SceneResponse::Error(scene_id, msg) => {
                        error!("[{scene_id:?}] error: {msg}");
                    }
                    SceneResponse::Ok(scene_id, census, mut crdt) => {
                        let root = updates.scene_ids.get(&scene_id).unwrap();
                        debug!(
                            "scene {:?}/{:?} received updates! [+{}, -{}]",
                            census.scene_id,
                            root,
                            census.born.len(),
                            census.died.len()
                        );
                        if let Ok(mut context) = scenes.get_mut(*root) {
                            context.in_flight = false;
                            context.nascent = census.born;
                            context.death_row = census.died;
                            let mut commands = commands.entity(*root);
                            for (component_id, interface) in crdt_interfaces.0.iter() {
                                interface.updates_to_entity(
                                    *component_id,
                                    &mut crdt,
                                    &mut commands,
                                );
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

        if Instant::now() > updates.loop_end_time {
            return;
        }
    }
}

#[derive(Component, Default)]
pub struct DeletedSceneEntities(pub HashSet<SceneEntityId>);

#[derive(Component)]
pub struct TargetParent(pub Entity);

fn process_lifecycle(
    mut commands: Commands,
    mut scenes: Query<(Entity, &mut RendererSceneContext, &mut DeletedSceneEntities)>,
    children: Query<&Children>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut handles: Local<Option<Handle<StandardMaterial>>>,
) {
    let material = handles.get_or_insert_with(|| materials.add(Color::WHITE.into()));

    for (root, mut context, mut deleted_entities) in scenes.iter_mut() {
        let scene_id = context.scene_id;
        if !context.nascent.is_empty() {
            debug!("{:?}: nascent: {:?}", root, context.nascent);
        }
        commands.entity(root).with_children(|child_builder| {
            for scene_entity_id in std::mem::take(&mut context.nascent) {
                if context.bevy_entity(scene_entity_id).is_some() {
                    continue;
                }
                context.associate_bevy_entity(
                    scene_entity_id,
                    child_builder
                        .spawn((
                            PbrBundle {
                                // TODO remove these and replace with spatial bundle when mesh and material components are supported
                                material: material.clone(),
                                ..Default::default()
                            },
                            SceneEntity {
                                scene_id,
                                root,
                                id: scene_entity_id,
                            },
                            TargetParent(root),
                        ))
                        .id(),
                );

                debug!(
                    "spawned {:?} -> {:?}",
                    scene_entity_id,
                    context.bevy_entity(scene_entity_id).unwrap()
                );
            }
        });

        // update deleted entities list, used by crdt processors to filter results
        deleted_entities.0 = std::mem::take(&mut context.death_row);

        for deleted_scene_entity in &deleted_entities.0 {
            if let Some(deleted_bevy_entity) = context.bevy_entity(*deleted_scene_entity) {
                // reparent children to the root entity
                if let Ok(children) = children.get(deleted_bevy_entity) {
                    commands.entity(root).push_children(children);
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
