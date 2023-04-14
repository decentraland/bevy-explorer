use std::path::PathBuf;

use bevy::{
    math::Vec3Swizzles,
    prelude::*,
    scene::scene_spawner_system,
    tasks::{IoTaskPool, Task},
    utils::{HashMap, HashSet},
};
use futures_lite::future;
use isahc::{http::StatusCode, AsyncReadResponseExt, RequestExt};
use serde::{Deserialize, Serialize};

use crate::{
    dcl::{interface::CrdtComponentInterfaces, spawn_scene},
    dcl_component::SceneEntityId,
    ipfs::{
        ipfs_path::{EntityType, IpfsPath},
        CurrentRealm, IpfsIo, IpfsLoaderExt, SceneDefinition, SceneIpfsLocation, SceneJsFile,
        SceneMeta,
    },
    scene_runner::{
        renderer_context::RendererSceneContext, DeletedSceneEntities, SceneEntity,
        SceneThreadHandle,
    },
};

use super::{update_world::CrdtExtractors, LoadSceneEvent, PrimaryCamera, SceneSets, SceneUpdates};

pub struct SceneLifecyclePlugin;

impl Plugin for SceneLifecyclePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LiveScenes>();
        app.init_resource::<ScenePointers>();
        app.insert_resource(SceneLoadDistance(100.0));

        app.add_systems(
            (
                load_scene_entity,
                load_scene_json,
                load_scene_javascript,
                initialize_scene,
            )
                .after(scene_spawner_system) // these can despawn scenes, make sure that the scene spawner system doesn't try to write to deleted entities
                .in_set(SceneSets::Init),
        );

        app.add_systems(
            (
                process_realm_change,
                load_active_entities,
                process_scene_lifecycle,
            )
                .chain()
                .in_base_set(CoreSet::PostUpdate),
        );
    }
}

#[derive(Component)]
pub enum SceneLoading {
    SceneSpawned,
    SceneEntity,
    SceneMeta,
    Javascript,
    Failed,
}

pub(crate) fn load_scene_entity(
    mut commands: Commands,
    mut load_scene_events: EventReader<LoadSceneEvent>,
    asset_server: Res<AssetServer>,
) {
    for event in load_scene_events.iter() {
        let mut commands = match event.entity {
            Some(entity) => {
                let Some(commands) = commands.get_entity(entity) else { continue; };
                commands
            }
            None => commands.spawn_empty(),
        };

        let h_scene = match &event.location {
            SceneIpfsLocation::Hash(hash) => {
                asset_server.load_hash::<SceneDefinition>(hash, EntityType::Scene)
            }
            SceneIpfsLocation::Urn(urn) => {
                match asset_server.load_urn::<SceneDefinition>(urn, EntityType::Scene) {
                    Ok(h_scene) => h_scene,
                    Err(e) => {
                        warn!("failed to parse urn: {e}");
                        commands.insert(SceneLoading::Failed);
                        continue;
                    }
                }
            }
        };

        commands.insert((SceneLoading::SceneEntity, h_scene));
    }
}

pub(crate) fn load_scene_json(
    mut commands: Commands,
    mut loading_scenes: Query<(Entity, &mut SceneLoading, &Handle<SceneDefinition>)>,
    scene_definitions: Res<Assets<SceneDefinition>>,
    asset_server: Res<AssetServer>,
) {
    for (entity, mut state, h_scene) in loading_scenes
        .iter_mut()
        .filter(|(_, state, _)| matches!(**state, SceneLoading::SceneEntity))
    {
        debug!("checking json");
        let mut fail = |msg: &str| {
            warn!("{entity:?} failed to initialize scene: {msg}");
            commands.entity(entity).insert(SceneLoading::Failed);
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

        let ipfs_io = asset_server.asset_io().downcast_ref::<IpfsIo>().unwrap();
        ipfs_io.add_collection(definition.id.clone(), definition.content.clone());

        let h_meta = match asset_server.load_content_file::<SceneMeta>("scene.json", &definition.id)
        {
            Ok(h_meta) => h_meta,
            Err(e) => {
                fail(&format!("couldn't load scene.json: {e}"));
                continue;
            }
        };

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
            commands.entity(entity).insert(SceneLoading::Failed);
        };

        match asset_server.get_load_state(h_meta) {
            bevy::asset::LoadState::Loaded => (),
            bevy::asset::LoadState::Failed => {
                fail("scene.json could not be loaded");
                continue;
            }
            _ => continue,
        }
        let Some(definition) = scene_definitions.get(h_scene) else {
            fail("definition was dropped");
            continue;
        };
        let Some(meta) = scene_metas.get(h_meta) else {
            fail("scene.json did not resolve to expected format");
            continue;
        };
        let h_code = match asset_server.load_content_file::<SceneJsFile>(&meta.main, &definition.id)
        {
            Ok(h_code) => h_code,
            Err(e) => {
                fail(&format!("couldn't load javascript: {e}"));
                continue;
            }
        };

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
            commands.entity(root).insert(SceneLoading::Failed);
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
pub struct LiveScenes(pub HashMap<String, Entity>);

pub const PARCEL_SIZE: f32 = 16.0;

#[derive(Resource, Default, Debug)]
pub struct ScenePointers(HashMap<IVec2, PointerResult>);

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum PointerResult {
    Nothing(i32, i32),
    Exists(String),
}

impl PointerResult {
    fn hash(&self) -> Option<&String> {
        match self {
            PointerResult::Nothing(..) => None,
            PointerResult::Exists(hash) => Some(hash),
        }
    }
}

fn parcels_in_range(focus: &GlobalTransform, range: f32) -> Vec<IVec2> {
    let focus = focus.translation().xz() * Vec2::new(1.0, -1.0);

    let min_point = focus - Vec2::splat(range);
    let max_point = focus + Vec2::splat(range);

    let min_parcel = (min_point / 16.0).floor().as_ivec2();
    let max_parcel = (max_point / 16.0).ceil().as_ivec2();

    let mut results = Vec::default();

    for parcel_x in min_parcel.x..=max_parcel.x {
        for parcel_y in min_parcel.y..=max_parcel.y {
            let parcel = IVec2::new(parcel_x, parcel_y);
            let parcel_min_point = parcel.as_vec2() * PARCEL_SIZE;
            let parcel_max_point = (parcel + 1).as_vec2() * PARCEL_SIZE;
            let nearest_point = focus.clamp(parcel_min_point, parcel_max_point);
            let distance = nearest_point.distance(focus);

            if distance < range {
                results.push(parcel);
            }
        }
    }

    results
}

pub fn process_realm_change(
    mut commands: Commands,
    current_realm: Res<CurrentRealm>,
    mut pointers: ResMut<ScenePointers>,
    mut live_scenes: ResMut<LiveScenes>,
    mut spawn: EventWriter<LoadSceneEvent>,
) {
    if current_realm.is_changed() {
        info!("realm change! purging scenes");
        let mut realm_scene_urns = HashSet::default();
        for urn in current_realm
            .config
            .scenes_urn
            .as_ref()
            .unwrap_or(&Vec::default())
        {
            let hacked_urn = urn.replace('?', "?=&");
            let path = match IpfsPath::new_from_urn(&hacked_urn, EntityType::Scene) {
                Ok(path) => path,
                Err(e) => {
                    warn!("failed to parse urn: `{}`: {}", urn, e);
                    continue;
                }
            };

            realm_scene_urns.insert((hacked_urn, path));
        }

        let realm_scene_ids = realm_scene_urns
            .into_iter()
            .flat_map(|(urn, path)| match path.context_free_hash() {
                Ok(Some(hash)) => Some((hash, urn)),
                otherwise => {
                    warn!("could not resolve hash from urn: {otherwise:?}");
                    None
                }
            })
            .collect::<HashMap<_, _>>();

        // purge pointers and scenes that are not in the realm list
        pointers.0.retain(|_, pr| match pr {
            PointerResult::Nothing(..) => false,
            PointerResult::Exists(hash) => realm_scene_ids.contains_key(hash),
        });
        live_scenes
            .0
            .retain(|hash, _| realm_scene_ids.contains_key(hash));

        // load the remaining unloaded scenes
        let to_load: HashSet<_> = realm_scene_ids
            .into_iter()
            .filter(|(hash, _)| !live_scenes.0.contains_key(hash))
            .collect();

        for (hash, urn) in to_load {
            let entity = commands.spawn(SceneLoading::SceneSpawned).id();
            info!("spawning scene {:?} @ ??: {entity:?}", hash);
            live_scenes.0.insert(hash, entity);
            spawn.send(LoadSceneEvent {
                entity: Some(entity),
                location: SceneIpfsLocation::Urn(urn),
            });
        }
    }
}

#[allow(clippy::type_complexity)]
fn load_active_entities(
    realm: Res<CurrentRealm>,
    focus: Query<&GlobalTransform, With<PrimaryCamera>>,
    range: Res<SceneLoadDistance>,
    mut pointers: ResMut<ScenePointers>,
    mut pointer_request: Local<
        Option<(
            HashSet<IVec2>,
            Task<Result<HashSet<ActiveEntity>, anyhow::Error>>,
        )>,
    >,
    asset_server: Res<AssetServer>,
) {
    if realm.is_changed() {
        // drop current request
        *pointer_request = None;
        return;
    }

    if pointer_request.is_none() {
        if let Some(url) = asset_server.active_endpoint() {
            // load required pointers
            let Ok(focus) = focus.get_single() else {
                return;
            };
            let parcels: HashSet<_> = parcels_in_range(focus, range.0)
                .into_iter()
                .filter(|parcel| !pointers.0.contains_key(parcel))
                .collect();

            if !parcels.is_empty() {
                let cache_path = asset_server.ipfs_cache_path().to_owned();
                *pointer_request = Some((
                    parcels.clone(),
                    IoTaskPool::get().spawn(request_active_entities(url, parcels, cache_path)),
                ));
            }
        }
    } else if pointer_request.as_ref().unwrap().1.is_finished() {
        // process active scenes in the requested set
        let (mut requested_parcels, mut task) = pointer_request.take().unwrap();

        let Ok(retrieved_parcels) = future::block_on(future::poll_once(&mut task)).unwrap() else {
            warn!("failed to retrieve active scenes, will retry");
            return;
        };

        info!(
            "found {} entities over parcels {:?}",
            retrieved_parcels.len(),
            requested_parcels
        );

        for active_entity in retrieved_parcels {
            for pointer in active_entity.pointers {
                let (x, y) = pointer.split_once(',').unwrap();
                let x = x.parse::<i32>().unwrap();
                let y = y.parse::<i32>().unwrap();
                let parcel = IVec2::new(x, y);

                requested_parcels.remove(&parcel);
                pointers
                    .0
                    .insert(parcel, PointerResult::Exists(active_entity.id.clone()));
            }
        }

        // any remaining requested parcels are empty
        for empty_parcel in requested_parcels {
            pointers.0.insert(
                empty_parcel,
                PointerResult::Nothing(empty_parcel.x, empty_parcel.y),
            );
        }
    }
}

// todo - this function is getting too big
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn process_scene_lifecycle(
    mut commands: Commands,
    current_realm: Res<CurrentRealm>,
    focus: Query<&GlobalTransform, With<PrimaryCamera>>,
    scene_entities: Query<Entity, Or<(With<SceneLoading>, With<RendererSceneContext>)>>,
    range: Res<SceneLoadDistance>,
    mut live_scenes: ResMut<LiveScenes>,
    mut updates: ResMut<SceneUpdates>,
    mut spawn: EventWriter<LoadSceneEvent>,
    pointers: Res<ScenePointers>,
) {
    let mut required_scene_ids: HashSet<String> = HashSet::default();

    // add realm-defined scenes to requirements
    if let Some(scenes) = current_realm.config.scenes_urn.as_ref() {
        required_scene_ids.extend(scenes.iter().flat_map(|urn| {
            let hacked_urn = urn.replace('?', "?=&");
            IpfsPath::new_from_urn(&hacked_urn, EntityType::Scene)
                .ok()?
                .context_free_hash()
                .ok()?
        }))
    }

    // otherwise add nearby scenes to requirements
    if required_scene_ids.is_empty() {
        let Ok(focus) = focus.get_single() else {
            return;
        };
        required_scene_ids.extend(parcels_in_range(focus, range.0).into_iter().flat_map(
            |parcel| {
                pointers
                    .0
                    .get(&parcel)
                    .and_then(PointerResult::hash)
                    .map(ToOwned::to_owned)
            },
        ));
    }

    // record which scene entities we should keep
    let required_entities: HashMap<_, _> = required_scene_ids
        .iter()
        .flat_map(|scene| live_scenes.0.get(scene).map(|ent| (ent, scene)))
        .collect();

    let mut existing_ids = HashSet::default();

    // despawn any no-longer required entities
    for entity in &scene_entities {
        match required_entities.get(&entity) {
            Some(hash) => {
                existing_ids.insert(<&String>::clone(hash));
            }
            None => {
                if let Some(commands) = commands.get_entity(entity) {
                    info!("despawning {:?}", entity);
                    commands.despawn_recursive();
                }

                // remove from running scenes
                updates.jobs_in_flight.remove(&entity);
            }
        }
    }
    drop(required_entities);

    // spawn any newly required scenes
    for required_scene_id in required_scene_ids
        .iter()
        .filter(|id| !existing_ids.contains(id))
    {
        let entity = commands.spawn(SceneLoading::SceneSpawned).id();
        info!("spawning scene {:?} @ ??: {entity:?}", required_scene_id);
        live_scenes.0.insert(required_scene_id.clone(), entity);
        spawn.send(LoadSceneEvent {
            entity: Some(entity),
            location: SceneIpfsLocation::Hash(required_scene_id.clone()),
        })
    }
}

#[derive(Serialize)]
struct ActiveEntitiesRequest {
    pointers: Vec<String>,
}

#[derive(Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ActiveEntity {
    id: String,
    pointers: Vec<String>,
}

#[derive(Deserialize, Debug)]
pub struct ActiveEntitiesResponse(Vec<serde_json::Value>);

async fn request_active_entities(
    url: String,
    pointers: HashSet<IVec2>,
    cache_path: PathBuf,
) -> Result<HashSet<ActiveEntity>, anyhow::Error> {
    let body = serde_json::to_string(&ActiveEntitiesRequest {
        pointers: pointers
            .into_iter()
            .map(|p| format!("{},{}", p.x, p.y))
            .collect(),
    })?;
    let mut response = isahc::Request::post(url)
        .header("content-type", "application/json")
        .body(body)?
        .send_async()
        .await?;

    if response.status() != StatusCode::OK {
        return Err(anyhow::anyhow!("status: {}", response.status()));
    }

    let active_entities = response
        .json::<ActiveEntitiesResponse>()
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    let mut res = HashSet::default();
    for entity in active_entities.0 {
        let id = entity
            .get("id")
            .ok_or(anyhow::anyhow!(
                "no id field on active entity: {:?}",
                entity
            ))?
            .as_str()
            .unwrap();
        // cache to file system
        let mut cache_path = cache_path.clone();
        cache_path.push(id);

        if id.starts_with("b64-") || !cache_path.exists() {
            let file = std::fs::File::create(&cache_path)?;
            serde_json::to_writer(file, &vec![&entity])?;
        }

        // return active entity struct
        res.insert(serde_json::from_value(entity)?);
    }

    Ok(res)
}