use bevy::{
    asset::{AssetLoader, LoadedAsset},
    math::Vec3Swizzles,
    prelude::*,
    reflect::{TypePath, TypeUuid},
    utils::{HashMap, HashSet},
};
use serde::Deserialize;

use common::util::{TaskExt, TryInsertEx};
use comms::global_crdt::GlobalCrdtState;
use dcl::{
    get_next_scene_id,
    interface::{crdt_context::CrdtContext, CrdtComponentInterfaces, CrdtType},
    spawn_scene, SceneElapsedTime, SceneResponse,
};
use dcl_component::{
    transform_and_parent::DclTransformAndParent, DclReader, DclWriter, SceneComponentId,
    SceneEntityId,
};
use ipfs::{
    ipfs_path::{EntityType, IpfsPath},
    ActiveEntityTask, CurrentRealm, EntityDefinition, IpfsLoaderExt, SceneIpfsLocation,
    SceneJsFile,
};

use super::{update_world::CrdtExtractors, LoadSceneEvent, PrimaryUser, SceneSets, SceneUpdates};
use crate::{
    renderer_context::RendererSceneContext, ContainerEntity, DeletedSceneEntities, SceneEntity,
    SceneThreadHandle,
};

pub struct CrdtLoader;

impl AssetLoader for CrdtLoader {
    fn load<'a>(
        &'a self,
        bytes: &'a [u8],
        load_context: &'a mut bevy::asset::LoadContext,
    ) -> bevy::utils::BoxedFuture<'a, anyhow::Result<(), anyhow::Error>> {
        Box::pin(async move {
            load_context.set_default_asset(LoadedAsset::new(SerializedCrdtStore(bytes.to_owned())));
            Ok(())
        })
    }

    fn extensions(&self) -> &[&str] {
        &["crdt"]
    }
}

pub struct SceneLifecyclePlugin;

impl Plugin for SceneLifecyclePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LiveScenes>();
        app.init_resource::<ScenePointers>();
        app.insert_resource(SceneLoadDistance(100.0));
        app.add_asset::<SerializedCrdtStore>();
        app.add_asset_loader(CrdtLoader);

        app.add_systems(
            Update,
            (
                load_scene_entity,
                load_scene_json,
                load_scene_javascript,
                initialize_scene,
            )
                .in_set(SceneSets::Init),
        );

        app.add_systems(
            PostUpdate,
            (
                process_realm_change,
                load_active_entities,
                process_scene_lifecycle,
            )
                .chain(),
        );
    }
}

#[derive(Component, Debug)]
pub enum SceneLoading {
    SceneSpawned,
    SceneEntity,
    MainCrdt(Option<Handle<SerializedCrdtStore>>),
    Javascript(Option<tokio::sync::broadcast::Receiver<Vec<u8>>>),
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
                asset_server.load_hash::<EntityDefinition>(hash, EntityType::Scene)
            }
            SceneIpfsLocation::Urn(urn) => {
                match asset_server.load_urn::<EntityDefinition>(urn, EntityType::Scene) {
                    Ok(h_scene) => h_scene,
                    Err(e) => {
                        warn!("failed to parse urn: {e}");
                        commands.try_insert(SceneLoading::Failed);
                        continue;
                    }
                }
            }
        };

        commands.try_insert((SceneLoading::SceneEntity, h_scene));
    }
}

pub(crate) fn load_scene_json(
    mut commands: Commands,
    mut loading_scenes: Query<(Entity, &mut SceneLoading, &Handle<EntityDefinition>)>,
    scene_definitions: Res<Assets<EntityDefinition>>,
    asset_server: Res<AssetServer>,
) {
    for (entity, mut state, h_scene) in loading_scenes
        .iter_mut()
        .filter(|(_, state, _)| matches!(**state, SceneLoading::SceneEntity))
    {
        let mut fail = |msg: &str| {
            warn!("{entity:?} failed to initialize scene: {msg}");
            commands.entity(entity).try_insert(SceneLoading::Failed);
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

        asset_server
            .ipfs()
            .add_collection(definition.id.clone(), definition.content.clone(), None);

        if definition.content.hash("main.crdt").is_some() {
            let h_crdt: Handle<SerializedCrdtStore> = asset_server
                .load_content_file("main.crdt", &definition.id)
                .unwrap();
            *state = SceneLoading::MainCrdt(Some(h_crdt));
        } else {
            *state = SceneLoading::MainCrdt(None);
        };
    }
}

#[derive(Deserialize, Debug)]
pub struct SceneMetaScene {
    pub base: String,
    pub parcels: Vec<String>,
}

#[derive(Deserialize, Debug)]
pub struct SceneDisplay {
    title: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct SceneMeta {
    pub display: Option<SceneDisplay>,
    pub main: String,
    pub scene: SceneMetaScene,
    #[serde(rename = "runtimeVersion")]
    pub runtime_version: Option<String>,
}

#[derive(TypeUuid, Default, Clone, TypePath)]
#[uuid = "e5f49bd0-15b0-43c1-8609-00bf8e1d23d4"]
pub struct SerializedCrdtStore(pub Vec<u8>);

#[allow(clippy::too_many_arguments)]
pub(crate) fn load_scene_javascript(
    mut commands: Commands,
    loading_scenes: Query<(Entity, &SceneLoading, &Handle<EntityDefinition>)>,
    scene_definitions: Res<Assets<EntityDefinition>>,
    main_crdts: Res<Assets<SerializedCrdtStore>>,
    asset_server: Res<AssetServer>,
    crdt_component_interfaces: Res<CrdtExtractors>,
    mut scene_updates: ResMut<SceneUpdates>,
    global_scene: Res<GlobalCrdtState>,
    mut pointers: ResMut<ScenePointers>,
) {
    for (root, state, h_scene) in loading_scenes
        .iter()
        .filter(|(_, state, _)| matches!(**state, SceneLoading::MainCrdt(_)))
    {
        let mut fail = |msg: &str| {
            warn!("{root:?} failed to initialize scene: {msg}");
            commands.entity(root).try_insert(SceneLoading::Failed);
        };

        let SceneLoading::MainCrdt(ref maybe_h_crdt) = state else { panic!("wrong load state in load_scene_javascript")};
        if let Some(ref h_crdt) = maybe_h_crdt {
            match asset_server.get_load_state(h_crdt) {
                bevy::asset::LoadState::Loaded => (),
                bevy::asset::LoadState::Failed => {
                    fail("scene.json could not be loaded");
                    continue;
                }
                _ => continue,
            }
        }

        let Some(definition) = scene_definitions.get(h_scene) else {
            fail("definition was dropped");
            continue;
        };
        let Some(meta) = &definition.metadata else {
            fail("definition didn't contain metadata");
            continue;
        };
        let Ok(meta) = serde_json::from_value::<SceneMeta>(meta.clone()) else {
            fail("scene.json did not resolve to expected format");
            continue;
        };

        // populate pointers
        for pointer in meta.scene.parcels {
            let (x, y) = pointer.split_once(',').unwrap();
            let x = x.parse::<i32>().unwrap();
            let y = y.parse::<i32>().unwrap();
            let parcel = IVec2::new(x, y);
            pointers
                .0
                .insert(parcel, PointerResult::Exists(definition.id.clone()));
        }

        // get main.crdt
        let maybe_serialized_crdt = match maybe_h_crdt {
            Some(ref h_crdt) => match main_crdts.get(h_crdt) {
                Some(crdt) => Some(crdt.clone().0),
                None => {
                    fail("failed to load crdt");
                    continue;
                }
            },
            None => None,
        };

        let is_sdk7 = match meta.runtime_version {
            Some(runtime_version) => runtime_version == "7",
            None => false
        };

        let h_code = if is_sdk7 {
            match asset_server.load_content_file::<SceneJsFile>(&meta.main, &definition.id) {
                Ok(h_code) => h_code,
                Err(e) => {
                    fail(&format!("couldn't load javascript: {}", e));
                    continue;
                }
            }
        } else {
            asset_server.load("sdk7_adaption_layer/index.js")
        };

        let crdt_component_interfaces = CrdtComponentInterfaces(HashMap::from_iter(
            crdt_component_interfaces
                .0
                .iter()
                .map(|(id, interface)| (*id, interface.crdt_type())),
        ));

        // spawn bevy-side scene
        let (pointer_x, pointer_y) = meta.scene.base.split_once(',').unwrap();
        let pointer_x = pointer_x.parse::<i32>().unwrap();
        let pointer_y = pointer_y.parse::<i32>().unwrap();
        let base = IVec2::new(pointer_x, pointer_y);

        let initial_position = base.as_vec2() * Vec2::splat(PARCEL_SIZE);

        // setup the scene root entity
        let scene_id = get_next_scene_id();
        let title = meta
            .display
            .and_then(|display| display.title)
            .unwrap_or("???".to_owned());
        let mut renderer_context =
            RendererSceneContext::new(scene_id, definition.id.clone(), title, base, root, 1.0);
        info!("{root:?}: started scene (location: {base:?}, scene thread id: {scene_id:?}, is sdk7: {is_sdk7:?})");

        scene_updates.scene_ids.insert(scene_id, root);

        // start from the global shared crdt state
        let (mut initial_crdt, global_updates) = global_scene.subscribe();

        // set the world origin (for parents of world-space entities, using world-space coords as local coords)
        let mut buf = Vec::new();
        DclWriter::new(&mut buf).write(&DclTransformAndParent::from_bevy_transform_and_parent(
            &Transform::from_translation(Vec3::new(-initial_position.x, 0.0, initial_position.y)),
            SceneEntityId::ROOT,
        ));
        initial_crdt.force_update(
            SceneComponentId::TRANSFORM,
            CrdtType::LWW_ANY,
            SceneEntityId::WORLD_ORIGIN,
            Some(&mut DclReader::new(&buf)),
        );

        if let Some(serialized_crdt) = maybe_serialized_crdt {
            // add main.crdt
            let mut context = CrdtContext::new(scene_id);
            let mut stream = DclReader::new(&serialized_crdt);
            initial_crdt.process_message_stream(
                &mut context,
                &crdt_component_interfaces,
                &mut stream,
                false,
            );

            // send initial updates into renderer
            let census = context.take_census();
            initial_crdt.clean_up(&census.died);
            let updates = initial_crdt.clone().take_updates();

            if let Err(e) = scene_updates.sender.send(SceneResponse::Ok(
                context.scene_id,
                census,
                updates,
                SceneElapsedTime(0.0),
                Default::default(),
            )) {
                error!("failed to send initial updates to renderer: {e}");
            }

            debug!("main crdt found for scene ent {root:?}");
        } else {
            // explicitly set initial tick as run
            renderer_context.tick_number = 1;
        }

        // store main.crdt + initial global state to post to the scene thread on first request
        renderer_context.crdt_store = initial_crdt;

        commands.entity(root).try_insert((
            SpatialBundle {
                transform: Transform::from_translation(Vec3::new(
                    initial_position.x,
                    0.0,
                    -initial_position.y,
                )),
                ..Default::default()
            },
            renderer_context,
            DeletedSceneEntities::default(),
            SceneEntity {
                root,
                scene_id,
                id: SceneEntityId::ROOT,
            },
            ContainerEntity {
                root,
                container: root,
                container_id: SceneEntityId::ROOT,
            },
        ));

        commands
            .entity(root)
            .try_insert((h_code, SceneLoading::Javascript(Some(global_updates))));
    }
}

#[allow(clippy::type_complexity)]
pub(crate) fn initialize_scene(
    mut commands: Commands,
    scene_updates: Res<SceneUpdates>,
    crdt_component_interfaces: Res<CrdtExtractors>,
    mut loading_scenes: Query<(
        Entity,
        &mut SceneLoading,
        &Handle<SceneJsFile>,
        &RendererSceneContext,
    )>,
    scene_js_files: Res<Assets<SceneJsFile>>,
    asset_server: Res<AssetServer>,
) {
    for (root, mut state, h_code, context) in loading_scenes.iter_mut() {
        if !matches!(state.as_mut(), SceneLoading::Javascript(_)) || context.tick_number != 1 {
            continue;
        }

        debug!("checking for js");
        let mut fail = |msg: &str| {
            warn!("{root:?} failed to initialize scene: {msg}");
            commands.entity(root).try_insert(SceneLoading::Failed);
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

        info!("{root:?}: starting scene sandbox");

        let thread_sx = scene_updates.sender.clone();

        let global_updates = match *state {
            SceneLoading::Javascript(ref mut global_updates) => global_updates.take(),
            _ => panic!("bad state"),
        }
        .unwrap();

        let crdt_component_interfaces = CrdtComponentInterfaces(HashMap::from_iter(
            crdt_component_interfaces
                .0
                .iter()
                .map(|(id, interface)| (*id, interface.crdt_type())),
        ));

        let scene_id = context.scene_id;
        let main_sx = spawn_scene(
            js_file.clone(),
            crdt_component_interfaces,
            thread_sx,
            global_updates,
            scene_id,
        );

        commands
            .entity(root)
            .try_insert((SceneThreadHandle { sender: main_sx },));
        commands.entity(root).remove::<SceneLoading>();
    }
}

#[derive(Resource)]
pub struct SceneLoadDistance(pub f32);

#[derive(Resource, Default)]
pub struct LiveScenes(pub HashMap<String, Entity>);

pub const PARCEL_SIZE: f32 = 16.0;

#[derive(Resource, Default, Debug)]
pub struct ScenePointers(pub HashMap<IVec2, PointerResult>);

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
    current_realm: Res<CurrentRealm>,
    mut pointers: ResMut<ScenePointers>,
    mut live_scenes: ResMut<LiveScenes>,
) {
    if current_realm.is_changed() {
        info!("realm change `{}`! purging scenes", current_realm.address);
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
    }
}

#[allow(clippy::type_complexity)]
fn load_active_entities(
    realm: Res<CurrentRealm>,
    focus: Query<&GlobalTransform, With<PrimaryUser>>,
    range: Res<SceneLoadDistance>,
    mut pointers: ResMut<ScenePointers>,
    mut pointer_request: Local<Option<(HashSet<IVec2>, ActiveEntityTask)>>,
    asset_server: Res<AssetServer>,
) {
    if realm.is_changed() {
        // drop current request
        *pointer_request = None;
    }

    if pointer_request.is_none() {
        let has_scene_urns = !realm.config.scenes_urn.as_ref().map_or(true, Vec::is_empty);
        if !has_scene_urns && asset_server.active_endpoint().is_some() {
            // load required pointers
            let Ok(focus) = focus.get_single() else {
                return;
            };

            let parcels: HashSet<_> = parcels_in_range(focus, range.0)
                .into_iter()
                .filter(|parcel| !pointers.0.contains_key(parcel))
                .collect();

            let pointers = parcels
                .iter()
                .map(|parcel| format!("{},{}", parcel.x, parcel.y))
                .collect();

            if !parcels.is_empty() {
                *pointer_request = Some((
                    parcels,
                    asset_server.ipfs().active_entities(&pointers, None),
                ));
            }
        }
    } else if let Some(task_result) = pointer_request.as_mut().unwrap().1.complete() {
        // process active scenes in the requested set
        let (mut requested_parcels, _) = pointer_request.take().unwrap();

        let Ok(retrieved_parcels) = task_result else {
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

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn process_scene_lifecycle(
    mut commands: Commands,
    current_realm: Res<CurrentRealm>,
    focus: Query<&GlobalTransform, With<PrimaryUser>>,
    scene_entities: Query<Entity, Or<(With<SceneLoading>, With<RendererSceneContext>)>>,
    range: Res<SceneLoadDistance>,
    mut live_scenes: ResMut<LiveScenes>,
    mut spawn: EventWriter<LoadSceneEvent>,
    pointers: Res<ScenePointers>,
) {
    let mut required_scene_ids: HashSet<(String, Option<String>)> = HashSet::default();

    // add realm-defined scenes to requirements
    if let Some(scenes) = current_realm.config.scenes_urn.as_ref() {
        required_scene_ids.extend(scenes.iter().flat_map(|urn| {
            let hacked_urn = urn.replace('?', "?=&");
            IpfsPath::new_from_urn(&hacked_urn, EntityType::Scene)
                .ok()?
                .context_free_hash()
                .ok()?
                .map(|hash| (hash, Some(hacked_urn)))
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
                    .map(|hash| (hash, None))
            },
        ));
    }

    // record which scene entities we should keep
    let required_entities: HashMap<_, _> = required_scene_ids
        .iter()
        .flat_map(|(hash, maybe_urn)| live_scenes.0.get(hash).map(|ent| (ent, (hash, maybe_urn))))
        .collect();

    let mut existing_ids = HashSet::default();

    // despawn any no-longer required entities
    for entity in &scene_entities {
        match required_entities.get(&entity) {
            Some((hash, _)) => {
                existing_ids.insert(<&String>::clone(hash));
            }
            None => {
                if let Some(commands) = commands.get_entity(entity) {
                    info!("despawning {:?}", entity);
                    commands.despawn_recursive();
                }
            }
        }
    }
    drop(required_entities);

    // spawn any newly required scenes
    for (required_scene_hash, maybe_urn) in required_scene_ids
        .iter()
        .filter(|(hash, _)| !existing_ids.contains(hash))
    {
        let entity = commands.spawn(SceneLoading::SceneSpawned).id();
        info!("spawning scene {:?} @ ??: {entity:?}", required_scene_hash);
        live_scenes.0.insert(required_scene_hash.clone(), entity);
        spawn.send(LoadSceneEvent {
            entity: Some(entity),
            location: match maybe_urn {
                Some(urn) => SceneIpfsLocation::Urn(urn.to_owned()),
                None => SceneIpfsLocation::Hash(required_scene_hash.to_owned()),
            },
        })
    }
}
