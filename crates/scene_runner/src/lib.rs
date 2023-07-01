use std::{
    collections::VecDeque,
    marker::PhantomData,
    sync::mpsc::{sync_channel, Receiver, SyncSender, TryRecvError},
    time::{Duration, SystemTime},
};

use bevy::{
    core::FrameCount,
    ecs::system::SystemParam,
    math::Vec3Swizzles,
    prelude::*,
    scene::scene_spawner_system,
    utils::{FloatOrd, HashMap, HashSet, Instant},
    window::PrimaryWindow,
    winit::WinitWindows,
};

use common::{
    sets::{SceneLoopSets, SceneSets},
    structs::{AppConfig, PrimaryCamera, PrimaryUser},
    util::{dcl_assert, TryInsertEx},
};
use dcl::{
    interface::CrdtType, RendererResponse, SceneId, SceneLogLevel, SceneLogMessage, SceneResponse,
};
use dcl_component::{
    proto_components::{common::BorderRect, sdk::components::PbUiCanvasInformation},
    transform_and_parent::DclTransformAndParent,
    DclReader, DclWriter, SceneComponentId, SceneEntityId,
};
use ipfs::SceneIpfsLocation;

use self::{
    initialize_scene::{
        LiveScenes, PointerResult, SceneLifecyclePlugin, SceneLoading, ScenePointers, PARCEL_SIZE,
    },
    renderer_context::RendererSceneContext,
    update_scene::SceneInputPlugin,
    update_world::{CrdtExtractors, SceneOutputPlugin},
};

pub mod initialize_scene;
pub mod renderer_context;
#[cfg(test)]
pub mod test;
pub mod update_scene;
pub mod update_world;

// bookkeeping struct for javascript execution of scenes
#[derive(Resource)]
pub struct SceneUpdates {
    pub sender: SyncSender<SceneResponse>,
    receiver: Receiver<SceneResponse>,
    pub scene_ids: HashMap<SceneId, Entity>,
    pub jobs_in_flight: HashSet<Entity>,
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
    pub entity: Option<Entity>,
    pub location: SceneIpfsLocation,
}

// this component is present on the bevy entity which maps to a scene entity explicitly
#[derive(Component, Debug)]
pub struct SceneEntity {
    pub root: Entity,
    pub scene_id: SceneId,
    pub id: SceneEntityId,
}

// this component is present on bevy entities which either
// - map to a scene entity
// - are owned by a scene entity
#[derive(Component, Debug)]
pub struct ContainerEntity {
    pub container: Entity,
    pub root: Entity,
    pub container_id: SceneEntityId,
}

// resource into which systems can add debug info
#[derive(Resource, Default, Debug)]
pub struct DebugInfo {
    pub info: HashMap<&'static str, String>,
}

// resource for adding toasts
#[derive(Resource, Default, Debug)]
pub struct Toasts(pub HashMap<&'static str, Toast>);

#[derive(SystemParam)]
pub struct Toaster<'w, 's> {
    toasts: ResMut<'w, Toasts>,
    time: Res<'w, Time>,
    #[system_param(ignore)]
    _p: PhantomData<&'s ()>,
}

impl<'w, 's> Toaster<'w, 's> {
    pub fn add_toast(&mut self, key: &'static str, message: impl Into<String>) {
        let message = message.into();
        if let Some(existing) = self.toasts.0.get(key) {
            if existing.message == message {
                return;
            }
        }

        self.toasts.0.insert(
            key,
            Toast {
                message,
                time: self.time.elapsed_seconds(),
            },
        );
    }

    pub fn clear_toast(&mut self, key: &'static str) {
        self.toasts.0.remove(key);
    }
}

#[derive(Debug)]
pub struct Toast {
    pub message: String,
    pub time: f32,
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
        app.init_resource::<DebugInfo>();
        app.init_resource::<Toasts>();

        let (sender, receiver) = sync_channel(1000);
        app.insert_resource(SceneUpdates {
            sender,
            receiver,
            scene_ids: Default::default(),
            jobs_in_flight: Default::default(),
            update_deadline: SystemTime::now(),
            eligible_jobs: 0,
            scene_queue: Default::default(),
            loop_end_time: Instant::now(),
        });

        app.add_event::<LoadSceneEvent>();

        app.configure_sets(
            (
                SceneSets::UiActions,
                SceneSets::Init.after(scene_spawner_system),
                SceneSets::PostInit,
                SceneSets::Input,
                SceneSets::RunLoop,
                SceneSets::PostLoop,
            )
                .in_base_set(CoreSet::Update)
                .chain(),
        );
        app.add_system(
            apply_system_buffers
                .after(SceneSets::UiActions)
                .before(SceneSets::Init),
        );
        app.add_system(
            apply_system_buffers
                .after(SceneSets::Init)
                .before(SceneSets::PostInit),
        );
        app.add_system(
            apply_system_buffers
                .after(SceneSets::PostInit)
                .before(SceneSets::Input),
        );
        app.add_system(
            apply_system_buffers
                .after(SceneSets::Input)
                .before(SceneSets::RunLoop),
        );
        app.add_system(
            apply_system_buffers
                .after(SceneSets::RunLoop)
                .before(SceneSets::PostLoop),
        );

        app.add_plugin(SceneLifecyclePlugin);

        app.add_systems(
            (update_scene_priority, run_scene_loop)
                .chain()
                .in_set(SceneSets::RunLoop),
        );

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
        scene_schedule.add_system(process_scene_entity_lifecycle.in_set(SceneLoopSets::Lifecycle));

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

        app.add_plugin(SceneInputPlugin);
        app.add_plugin(SceneOutputPlugin);
    }
}

fn run_scene_loop(world: &mut World) {
    let mut window_query = world.query_filtered::<Entity, With<PrimaryWindow>>();
    let winit_windows = world.get_non_send_resource::<WinitWindows>();
    let refresh_rate = window_query
        .get_single(&world)
        .ok()
        .and_then(|window_ent| winit_windows.and_then(|ww| ww.get_window(window_ent)))
        .and_then(|window| window.current_monitor())
        .and_then(|monitor| monitor.refresh_rate_millihertz());
    let config = world.resource::<AppConfig>();
    let fps = if config.graphics.vsync {
        // TODO this should use video mode if we add fullscreen video modes
        refresh_rate
            .map(|rr| (rr / 1000) as usize)
            .unwrap_or(config.graphics.fps_target)
    } else {
        config.graphics.fps_target
    };
    let mut loop_schedule = world.resource_mut::<SceneLoopSchedule>();
    let mut schedule = std::mem::take(&mut loop_schedule.schedule);
    let target_end_time =
        loop_schedule.end_time + Duration::from_nanos((1000_000_000.0 / fps as f32) as u64);
    let target_end_time = target_end_time.max(Instant::now() + Duration::from_millis(1));

    world.resource_mut::<SceneUpdates>().loop_end_time = target_end_time;

    // run at least once to collect updates even if no scenes are eligible
    let mut run_once = false;

    // run until time elapsed or all scenes are updated
    while !run_once
        || (Instant::now() < target_end_time
            && (world.resource::<SceneUpdates>().eligible_jobs > 0
                || !world.resource::<SceneUpdates>().jobs_in_flight.is_empty()))
    {
        schedule.run(world);
        run_once = true;
    }

    let mut loop_schedule = world.resource_mut::<SceneLoopSchedule>();
    loop_schedule.schedule = schedule;

    let actual_end_time = Instant::now();
    loop_schedule.end_time = target_end_time.max(actual_end_time);

    if let Some(sleep_time) = target_end_time.checked_duration_since(actual_end_time) {
        spin_sleep::sleep(sleep_time)
    }
}

fn update_scene_priority(
    mut scenes: Query<(Entity, &GlobalTransform, &mut RendererSceneContext), Without<SceneLoading>>,
    player: Query<(Entity, &GlobalTransform), With<PrimaryUser>>,
    mut updates: ResMut<SceneUpdates>,
    time: Res<Time>,
    containing_scene: ContainingScene,
) {
    updates.eligible_jobs = 0;

    let (player_scene, player_translation) = player
        .get_single()
        .map(|(e, gt)| (containing_scene.get(e), gt.translation()))
        .unwrap_or_default();

    // check all in-flight scenes still exist
    let mut missing_in_flight = updates.jobs_in_flight.clone();

    // sort eligible scenes
    updates.scene_queue = scenes
        .iter_mut()
        .filter(|(ent, _, context)| {
            missing_in_flight.remove(ent);
            !context.in_flight && !context.broken && context.blocked.is_empty()
        })
        .filter_map(|(ent, transform, mut context)| {
            // TODO clamp to scene bounds instead of using distance to scene origin
            let distance = (transform.translation() - player_translation).length();
            context.priority = if Some(ent) == player_scene {
                0.0
            } else {
                distance
            };
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

    // remove any scenes we didn't see from the in-flight set
    updates.jobs_in_flight = &updates.jobs_in_flight - &missing_in_flight;
}

// TODO: work out how to set this intelligently
// we need to keep enough scheduler time to ensure the main loop wakes enough
// otherwise we end up overrunning the budget
// also consider
// - reduce bevy async thread pool
// - reduce bevy primary thread pool
// - see if we can get v8 single threaded / no native threads working
// const MAX_CONCURRENT_SCENES: usize = 8;

// helper to get the scene entity containing a given world position
#[derive(SystemParam)]
pub struct ContainingScene<'w, 's> {
    transforms: Query<'w, 's, &'static GlobalTransform>,
    pointers: Res<'w, ScenePointers>,
    live_scenes: Res<'w, LiveScenes>,
}

impl<'w, 's> ContainingScene<'w, 's> {
    pub fn get(&self, ent: Entity) -> Option<Entity> {
        let parcel = (self.transforms.get(ent).ok()?.translation().xz() * Vec2::new(1.0, -1.0)
            / PARCEL_SIZE)
            .floor()
            .as_ivec2();

        if let Some(PointerResult::Exists(hash)) = self.pointers.0.get(&parcel) {
            self.live_scenes.0.get(hash).copied()
        } else {
            None
        }
    }

    // get all scenes within radius of the given entity
    pub fn get_area(&self, ent: Entity, radius: f32) -> Vec<Entity> {
        let Ok(focus) = self.transforms.get(ent).map(|t| t.translation().xz() * Vec2::new(1.0, -1.0)) else {
            return Default::default();
        };

        let min_point = focus - Vec2::splat(radius);
        let max_point = focus + Vec2::splat(radius);

        let min_parcel = (min_point / PARCEL_SIZE).floor().as_ivec2();
        let max_parcel = (max_point / PARCEL_SIZE).floor().as_ivec2();

        let mut results = Vec::default();

        for parcel_x in min_parcel.x..=max_parcel.x {
            for parcel_y in min_parcel.y..=max_parcel.y {
                if let Some(PointerResult::Exists(hash)) =
                    self.pointers.0.get(&IVec2::new(parcel_x, parcel_y))
                {
                    if let Some(scene) = self.live_scenes.0.get(hash).copied() {
                        results.push(scene)
                    }
                }
            }
        }

        results
    }
}

fn send_scene_updates(
    mut scenes: Query<(
        Entity,
        &mut RendererSceneContext,
        &SceneThreadHandle,
        &GlobalTransform,
    )>,
    mut updates: ResMut<SceneUpdates>,
    time: Res<Time>,
    player: Query<&Transform, With<PrimaryUser>>,
    camera: Query<&Transform, With<PrimaryCamera>>,
    config: Res<AppConfig>,
    window: Query<&Window, With<PrimaryWindow>>,
) {
    let updates = &mut *updates;

    if updates.jobs_in_flight.len() == config.scene_threads {
        return;
    }

    let Some((ent, _)) = updates.scene_queue.pop_front() else {
        return;
    };

    let (_, mut context, handle, scene_transform) = scenes.get_mut(ent).unwrap();

    // collect components

    // generate updates for camera and player
    let crdt_store = &mut context.crdt_store;

    let mut buf = Vec::default();
    for (mut affine, id) in [
        (player.single().compute_affine(), SceneEntityId::PLAYER),
        (camera.single().compute_affine(), SceneEntityId::CAMERA),
    ] {
        buf.clear();
        affine.translation -= scene_transform.affine().translation;
        let relative_transform = Transform::from(GlobalTransform::from(affine));

        DclWriter::new(&mut buf).write(&DclTransformAndParent::from_bevy_transform_and_parent(
            &relative_transform,
            SceneEntityId::ROOT,
        ));

        crdt_store.force_update(
            SceneComponentId::TRANSFORM,
            CrdtType::LWW_ENT,
            id,
            Some(&mut DclReader::new(&buf)),
        );
    }

    // add canvas info
    if let Ok(window) = window.get_single() {
        buf.clear();
        DclWriter::new(&mut buf).write(&PbUiCanvasInformation {
            device_pixel_ratio: window.resolution.scale_factor() as f32,
            width: window.resolution.width() as i32,
            height: window.resolution.height() as i32,
            interactable_area: Some(BorderRect {
                top: 0.0,
                left: 0.0,
                right: 0.0,
                bottom: 0.0,
            }),
        });
        crdt_store.force_update(
            SceneComponentId::CANVAS_INFO,
            CrdtType::LWW_ROOT,
            SceneEntityId::ROOT,
            Some(&mut DclReader::new(&buf)),
        );
    }

    if let Err(e) = handle
        .sender
        .blocking_send(RendererResponse::Ok(crdt_store.take_updates()))
    {
        error!(
            "failed to send updates to scene {ent:?} [{:?}]: {e:?}",
            context.base
        );
        context.broken = true;
        // TODO: clean up
    } else {
        context.in_flight = true;
        context.last_sent = time.elapsed_seconds();
        dcl_assert!(!updates.jobs_in_flight.contains(&ent));
        updates.jobs_in_flight.insert(ent);
    }

    updates.eligible_jobs -= 1;
}

// system to run the current active script
fn receive_scene_updates(
    mut commands: Commands,
    mut updates: ResMut<SceneUpdates>,
    mut scenes: Query<&mut RendererSceneContext>,
    crdt_interfaces: Res<CrdtExtractors>,
    frame: Res<FrameCount>,
) {
    loop {
        let maybe_completed_job = match updates.receiver().try_recv() {
            Ok(response) => match response {
                SceneResponse::Error(scene_id, message) => {
                    error!("[{scene_id:?}] error: {message}");
                    if let Some(root) = updates.scene_ids.get(&scene_id) {
                        if let Ok(mut context) = scenes.get_mut(*root) {
                            context.broken = true;
                            context.in_flight = false;
                            let timestamp = context.total_runtime as f64 + 1.0;
                            context.logs.send(SceneLogMessage {
                                timestamp,
                                level: SceneLogLevel::SystemError,
                                message,
                            });
                        }
                        Some(*root)
                    } else {
                        None
                    }
                }
                SceneResponse::Ok(scene_id, census, mut crdt, runtime, messages) => {
                    let root = updates.scene_ids.get(&scene_id).unwrap();
                    debug!(
                        "scene {:?}/{:?} received updates! [+{}, -{}]",
                        census.scene_id,
                        root,
                        census.born.len(),
                        census.died.len()
                    );
                    if let Ok(mut context) = scenes.get_mut(*root) {
                        context.tick_number = context.tick_number.wrapping_add(1);
                        context.last_update_dt = runtime.0 - context.total_runtime;
                        context.total_runtime = runtime.0;
                        context.last_update_frame = frame.0;
                        context.in_flight = false;
                        context.nascent = census.born;
                        context.death_row = census.died;
                        for message in messages.into_iter() {
                            context.logs.send(message);
                        }
                        let mut commands = commands.entity(*root);
                        for (component_id, interface) in crdt_interfaces.0.iter() {
                            interface.updates_to_entity(*component_id, &mut crdt, &mut commands);
                        }
                        dcl_assert!(
                            updates.jobs_in_flight.contains(root) || context.tick_number == 1
                        );
                    } else {
                        debug!(
                            "no scene entity, probably got dropped before we processed the result"
                        );
                    }
                    Some(*root)
                }
            },
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                panic!("render thread receiver exploded");
            }
        };

        if let Some(completed_job) = maybe_completed_job {
            updates.jobs_in_flight.remove(&completed_job);
        }

        if Instant::now() > updates.loop_end_time {
            return;
        }
    }
}

// entities deleted this loop
// note this is only valid within the scene loop, as it is overwritten in each lifecycle update (within the loop)
#[derive(Component, Default)]
pub struct DeletedSceneEntities(pub HashSet<SceneEntityId>);

#[derive(Component)]
pub struct TargetParent(pub Entity);

fn process_scene_entity_lifecycle(
    mut commands: Commands,
    mut scenes: Query<(Entity, &mut RendererSceneContext, &mut DeletedSceneEntities)>,
    children: Query<&Children>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut handles: Local<Option<Handle<StandardMaterial>>>,
    scene_entities: Query<(), With<SceneEntity>>,
) {
    let material = handles.get_or_insert_with(|| materials.add(Color::WHITE.into()));

    for (root, mut context, mut deleted_entities) in scenes.iter_mut() {
        let scene_id = context.scene_id;
        if !context.nascent.is_empty() {
            debug!("{:?}: nascent: {:?}", root, context.nascent);
        }

        for scene_entity_id in std::mem::take(&mut context.nascent) {
            if context.bevy_entity(scene_entity_id).is_some() {
                continue;
            }

            let spawned = commands
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
                .id();

            commands.entity(spawned).try_insert(ContainerEntity {
                root,
                container: spawned,
                container_id: scene_entity_id,
            });

            commands.entity(root).add_child(spawned);

            context.associate_bevy_entity(scene_entity_id, spawned);

            debug!(
                "spawned {:?} -> {:?}",
                scene_entity_id,
                context.bevy_entity(scene_entity_id).unwrap()
            );
        }

        // update deleted entities list, used by crdt processors to filter results
        deleted_entities.0 = std::mem::take(&mut context.death_row);

        for deleted_scene_entity in &deleted_entities.0 {
            if let Some(deleted_bevy_entity) = context.bevy_entity(*deleted_scene_entity) {
                // reparent scene-entity children to the root entity
                if let Ok(children) = children.get(deleted_bevy_entity) {
                    let scene_children = children
                        .iter()
                        .filter(|child| scene_entities.get(**child).is_ok())
                        .copied()
                        .collect::<Vec<_>>();
                    commands
                        .entity(root)
                        .push_children(scene_children.as_slice());
                }

                debug!(
                    "despawned {:?} -> {:?}",
                    deleted_scene_entity, deleted_bevy_entity
                );
                commands.entity(deleted_bevy_entity).despawn_recursive();
            }
            context.set_dead(*deleted_scene_entity);
        }
    }
}
