use bevy::{
    math::Vec3Swizzles,
    prelude::*,
    utils::{HashMap, HashSet},
};

use crate::{
    dcl::{interface::CrdtComponentInterfaces, spawn_scene},
    dcl_component::SceneEntityId,
    ipfs::{IpfsIo, IpfsLoaderExt, SceneDefinition, SceneIpfsLocation, SceneJsFile, SceneMeta},
    scene_runner::{
        renderer_context::RendererSceneContext, DeletedSceneEntities, SceneEntity,
        SceneThreadHandle,
    },
};

use super::{update_world::CrdtExtractors, LoadSceneEvent, PrimaryCamera, SceneUpdates};

#[derive(Component)]
pub enum SceneLoading {
    SceneEntity,
    SceneMeta,
    Javascript,
}

pub(crate) fn load_scene_entity(
    mut commands: Commands,
    mut load_scene_events: EventReader<LoadSceneEvent>,
    mut live_scenes: ResMut<LiveScenes>,
    asset_server: Res<AssetServer>,
) {
    for event in load_scene_events.iter() {
        match &event.location {
            SceneIpfsLocation::Pointer(x, y) => {
                let ent = commands
                    .spawn((
                        SceneLoading::SceneEntity,
                        asset_server.load_scene_pointer(*x, *y),
                    ))
                    .id();
                live_scenes.0.insert(IVec2::new(*x, *y), ent);
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

pub(crate) fn load_scene_json(
    mut commands: Commands,
    mut loading_scenes: Query<(Entity, &mut SceneLoading, &Handle<SceneDefinition>)>,
    scene_definitions: Res<Assets<SceneDefinition>>,
    asset_server: Res<AssetServer>,
    mut live_scenes: ResMut<LiveScenes>,
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

        if definition.id.is_empty() {
            // there was nothing at this pointer
            // stop loading but don't despawn
            commands.entity(entity).remove::<SceneLoading>();
            continue;
        }

        // scene entity is invalid if all live_scene pointers are present and refer to another entity
        let is_invalid = definition.pointers.iter().all(|pointer| {
            live_scenes
                .0
                .get(pointer)
                .map(|live_entity| live_entity != &entity)
                .unwrap_or(false)
        });

        if is_invalid {
            commands.entity(entity).despawn_recursive();
            continue;
        }

        // otherwise either the live scenes don't contain anything (e.g. if loaded via entity ref or js filename)
        // or at least one scene pointer contains this entity. in that case, we stamp our authority on all
        // the parcels, which will cause all other entities referenced in those parcels to despawn when they
        // reach the invalid test above.
        for pointer in &definition.pointers {
            live_scenes.0.insert(*pointer, entity);
        }

        let ipfs_io = asset_server.asset_io().downcast_ref::<IpfsIo>().unwrap();
        ipfs_io.add_collection(definition.id.clone(), definition.content.clone());

        let h_meta = asset_server
            .load_content_file::<SceneMeta>("scene.json".to_owned(), definition.id.to_owned());

        commands.entity(entity).insert(h_meta);
        *state = SceneLoading::SceneMeta;
    }
}

pub(crate) fn load_scene_javascript(
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
        let h_code = asset_server
            .load_content_file::<SceneJsFile>(meta.main.to_owned(), definition.id.to_owned());

        commands.entity(entity).insert(h_code);
        *state = SceneLoading::Javascript;
    }
}

#[allow(clippy::type_complexity)]
pub(crate) fn initialize_scene(
    mut commands: Commands,
    mut scene_updates: ResMut<SceneUpdates>,
    crdt_component_interfaces: Res<CrdtExtractors>,
    loading_scenes: Query<(
        Entity,
        &SceneLoading,
        &Handle<SceneJsFile>,
        Option<&Handle<SceneMeta>>,
    )>,
    scene_js_files: Res<Assets<SceneJsFile>>,
    scene_metas: Res<Assets<SceneMeta>>,
    asset_server: Res<AssetServer>,
) {
    for (root, _, h_code, maybe_h_meta) in loading_scenes
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

        let base = match maybe_h_meta {
            Some(h_meta) => {
                let meta = scene_metas.get(h_meta).unwrap();

                let (pointer_x, pointer_y) = meta.scene.base.split_once(',').unwrap();
                let pointer_x = pointer_x.parse::<i32>().unwrap();
                let pointer_y = pointer_y.parse::<i32>().unwrap();
                IVec2::new(pointer_x, pointer_y)
            }
            None => Default::default(),
        };

        let initial_position = base.as_vec2() * Vec2::splat(PARCEL_SIZE);

        // setup the scene root entity
        commands.entity(root).remove::<SceneLoading>().insert((
            SpatialBundle {
                transform: Transform::from_translation(Vec3::new(
                    initial_position.x,
                    0.0,
                    -initial_position.y,
                )),
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

        let renderer_context = RendererSceneContext::new(scene_id, base, root, 1.0);
        info!("{root:?}: started scene (location: {base:?}, scene thread id: {scene_id:?})");

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

#[derive(Resource)]
pub struct SceneLoadDistance(pub f32);

#[derive(Resource, Default)]
pub struct LiveScenes(pub HashMap<IVec2, Entity>);

pub const PARCEL_SIZE: f32 = 16.0;

#[allow(clippy::type_complexity)]
pub fn process_scene_lifecycle(
    mut commands: Commands,
    focus: Query<&GlobalTransform, With<PrimaryCamera>>,
    range: Res<SceneLoadDistance>,
    mut live_scenes: ResMut<LiveScenes>,
    mut spawn: EventWriter<LoadSceneEvent>,
    mut updates: ResMut<SceneUpdates>,
    scene_entities: Query<(), Or<(With<SceneLoading>, With<RendererSceneContext>)>>,
) {
    let Ok(focus) = focus.get_single() else {
        return;
    };
    let focus = focus.translation().xz() * Vec2::new(1.0, -1.0);

    let min_point = focus - Vec2::splat(range.0);
    let max_point = focus + Vec2::splat(range.0);

    let min_parcel = (min_point / 16.0).floor().as_ivec2();
    let max_parcel = (max_point / 16.0).ceil().as_ivec2();

    let mut good_scenes = HashSet::default();

    // iterate parcels within range
    for parcel_x in min_parcel.x..=max_parcel.x {
        for parcel_y in min_parcel.y..=max_parcel.y {
            let parcel = IVec2::new(parcel_x, parcel_y);
            let parcel_min_point = parcel.as_vec2() * PARCEL_SIZE;
            let parcel_max_point = (parcel + 1).as_vec2() * PARCEL_SIZE;
            let nearest_point = focus.clamp(parcel_min_point, parcel_max_point);
            let distance = nearest_point.distance(focus);

            if distance < range.0 {
                if let Some(scene_entity) = live_scenes.0.get(&parcel) {
                    // record still-valid entities
                    if scene_entities.get(*scene_entity).is_ok() {
                        good_scenes.insert(*scene_entity);
                    }
                } else {
                    // or spawn them in
                    info!("spawning scene @ {:?}", parcel);
                    spawn.send(LoadSceneEvent {
                        location: SceneIpfsLocation::Pointer(parcel.x, parcel.y),
                    });
                }
            }
        }
    }

    // despawn any no-longer valid scenes
    live_scenes.0.retain(|_, entity| {
        let is_good = good_scenes.contains(entity);
        if !is_good {
            if let Some(commands) = commands.get_entity(*entity) {
                info!("despawning scene {:?}", entity);
                commands.despawn_recursive();
            }

            // remove from running scenes
            updates.jobs_in_flight.remove(entity);
        }
        is_good
    })
}
