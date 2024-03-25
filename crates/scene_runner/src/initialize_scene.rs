use std::{collections::VecDeque, num::ParseIntError, str::FromStr};

use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    math::Vec3Swizzles,
    prelude::*,
    reflect::TypePath,
    utils::{BoxedFuture, HashMap, HashSet},
};
use futures_lite::AsyncReadExt;

use common::{
    structs::{AppConfig, IVec2Arg, SceneLoadDistance, SceneMeta},
    util::TaskExt,
};
use comms::global_crdt::GlobalCrdtState;
use dcl::{
    interface::{crdt_context::CrdtContext, CrdtComponentInterfaces, CrdtType},
    spawn_scene, SceneElapsedTime, SceneId, SceneResponse,
};
use dcl_component::{
    transform_and_parent::DclTransformAndParent, DclReader, DclWriter, SceneComponentId,
    SceneEntityId,
};
use ipfs::{
    ipfs_path::IpfsPath, ActiveEntityTask, CurrentRealm, EntityDefinition, IpfsAssetServer,
    IpfsResource, SceneIpfsLocation, SceneJsFile,
};
use wallet::Wallet;

use super::{update_world::CrdtExtractors, LoadSceneEvent, PrimaryUser, SceneSets, SceneUpdates};
use crate::{
    renderer_context::RendererSceneContext, update_world::ComponentTracker, ContainerEntity,
    DeletedSceneEntities, SceneEntity, SceneThreadHandle,
};

#[derive(Default)]
pub struct CrdtLoader;

impl AssetLoader for CrdtLoader {
    type Asset = SerializedCrdtStore;
    type Error = std::io::Error;
    type Settings = ();

    fn load<'a>(
        &'a self,
        reader: &'a mut Reader,
        _: &'a Self::Settings,
        _: &'a mut LoadContext,
    ) -> BoxedFuture<'a, Result<Self::Asset, Self::Error>> {
        Box::pin(async move {
            let mut bytes = Vec::default();
            reader.read_to_end(&mut bytes).await?;
            Ok(SerializedCrdtStore(bytes))
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
        app.init_resource::<PortableScenes>();
        app.init_asset::<SerializedCrdtStore>();
        app.init_asset_loader::<CrdtLoader>();

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
    SceneEntity {
        realm: String,
    },
    MainCrdt {
        crdt: Option<Handle<SerializedCrdtStore>>,
    },
    Javascript(Option<tokio::sync::broadcast::Receiver<Vec<u8>>>),
    Failed,
}

pub(crate) fn load_scene_entity(
    mut commands: Commands,
    mut load_scene_events: EventReader<LoadSceneEvent>,
    ipfas: IpfsAssetServer,
) {
    for event in load_scene_events.read() {
        let mut commands = match event.entity {
            Some(entity) => {
                let Some(commands) = commands.get_entity(entity) else {
                    continue;
                };
                commands
            }
            None => commands.spawn_empty(),
        };

        let h_scene = match &event.location {
            SceneIpfsLocation::Hash(hash) => ipfas.load_hash::<EntityDefinition>(hash),
            SceneIpfsLocation::Urn(urn) => match ipfas.load_urn::<EntityDefinition>(urn) {
                Ok(h_scene) => h_scene,
                Err(e) => {
                    warn!("failed to parse urn: {e}");
                    commands.try_insert(SceneLoading::Failed);
                    continue;
                }
            },
        };

        commands.try_insert((
            SceneLoading::SceneEntity {
                realm: event.realm.clone(),
            },
            h_scene,
        ));
    }
}

pub(crate) fn load_scene_json(
    mut commands: Commands,
    mut loading_scenes: Query<(Entity, &mut SceneLoading, &Handle<EntityDefinition>)>,
    scene_definitions: Res<Assets<EntityDefinition>>,
    ipfas: IpfsAssetServer,
) {
    for (entity, mut state, h_scene) in loading_scenes
        .iter_mut()
        .filter(|(_, state, _)| matches!(**state, SceneLoading::SceneEntity { .. }))
    {
        let mut fail = |msg: &str| {
            warn!("{entity:?} failed to initialize scene: {msg}");
            commands.entity(entity).try_insert(SceneLoading::Failed);
        };

        match ipfas.load_state(h_scene) {
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

        ipfas.ipfs().add_collection(
            definition.id.clone(),
            definition.content.clone(),
            None,
            definition.metadata.as_ref().map(|v| v.to_string()),
        );

        let crdt = definition.content.hash("main.crdt").map(|_| {
            ipfas
                .load_content_file("main.crdt", &definition.id)
                .unwrap()
        });
        *state = SceneLoading::MainCrdt { crdt };
    }
}

#[derive(Asset, Default, Clone, TypePath)]
pub struct SerializedCrdtStore(pub Vec<u8>);

#[allow(clippy::too_many_arguments)]
pub(crate) fn load_scene_javascript(
    mut commands: Commands,
    config: Res<AppConfig>,
    loading_scenes: Query<(Entity, &SceneLoading, &Handle<EntityDefinition>)>,
    scene_definitions: Res<Assets<EntityDefinition>>,
    main_crdts: Res<Assets<SerializedCrdtStore>>,
    ipfas: IpfsAssetServer,
    crdt_component_interfaces: Res<CrdtExtractors>,
    mut scene_updates: ResMut<SceneUpdates>,
    global_scene: Res<GlobalCrdtState>,
    portable_scenes: Res<PortableScenes>,
) {
    for (root, state, h_scene) in loading_scenes
        .iter()
        .filter(|(_, state, _)| matches!(**state, SceneLoading::MainCrdt { .. }))
    {
        let mut fail = |msg: &str| {
            warn!("{root:?} failed to initialize scene: {msg}");
            commands.entity(root).try_insert(SceneLoading::Failed);
        };

        let SceneLoading::MainCrdt { ref crdt } = state else {
            panic!("wrong load state in load_scene_javascript")
        };
        if let Some(ref h_crdt) = crdt {
            match ipfas.load_state(h_crdt) {
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

        let is_portable = portable_scenes.0.contains_key(&definition.id);

        let (base_x, base_y) = meta.scene.base.split_once(',').unwrap();
        let base_x = base_x.parse::<i32>().unwrap();
        let base_y = base_y.parse::<i32>().unwrap();
        let base = IVec2::new(base_x, base_y);

        // populate pointers
        let mut extent_min = IVec2::MAX;
        let mut extent_max = IVec2::MIN;
        for pointer in meta.scene.parcels {
            let (x, y) = pointer.split_once(',').unwrap();
            let x = x.parse::<i32>().unwrap();
            let y = y.parse::<i32>().unwrap();
            let parcel = IVec2::new(x, y);

            extent_min = extent_min.min(parcel);
            extent_max = extent_max.max(parcel);
        }
        let size = (extent_max - extent_min).as_uvec2();
        let bounds = IVec4::new(
            extent_min.x * 16,
            -(extent_max.y + 1) * 16,
            (extent_max.x + 1) * 16,
            -extent_min.y * 16,
        )
        .as_vec4();

        // get main.crdt
        let maybe_serialized_crdt = match crdt {
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
            None => false,
        };

        let h_code = if is_sdk7 {
            match ipfas.load_content_file::<SceneJsFile>(&meta.main, &definition.id) {
                Ok(h_code) => h_code,
                Err(e) => {
                    fail(&format!("couldn't load javascript: {}", e));
                    continue;
                }
            }
        } else {
            ipfas.load_url(
                "https://renderer-artifacts.decentraland.org/sdk7-adaption-layer/dev/index.min.js",
            )
        };

        let crdt_component_interfaces = CrdtComponentInterfaces(HashMap::from_iter(
            crdt_component_interfaces
                .0
                .iter()
                .map(|(id, interface)| (*id, interface.crdt_type())),
        ));

        // spawn bevy-side scene
        let initial_position = base.as_vec2() * Vec2::splat(PARCEL_SIZE);

        // setup the scene root entity
        let scene_id = SceneId(root);
        let title = meta
            .display
            .and_then(|display| display.title)
            .unwrap_or("???".to_owned());
        let mut renderer_context = RendererSceneContext::new(
            scene_id,
            definition.id.clone(),
            is_portable,
            title,
            base,
            bounds,
            meta.spawn_points.clone().unwrap_or_default(),
            root,
            size,
            1.0,
            config.scene_log_to_console,
            if is_sdk7 { "sdk7" } else { "sdk6" },
        );
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
            let mut context = CrdtContext::new(scene_id, renderer_context.hash.clone(), false);
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
            ComponentTracker::default(),
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

#[derive(Clone)]
pub struct TestScenes(pub VecDeque<TestScene>);

#[derive(Clone)]
pub struct TestScene {
    pub location: IVec2,
    pub allow_failures: Vec<String>,
}

impl FromStr for TestScenes {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let scenes: Result<VecDeque<TestScene>, ParseIntError> = value
            .split(';')
            .map(|scene| {
                println!("parsing test scenes scene {}", scene);
                if let Some((parcel, fails)) = scene.split_once('/') {
                    let allow_failures = fails.split('/').map(ToOwned::to_owned).collect();
                    println!("allowed failures: {allow_failures:?}");
                    Ok(TestScene {
                        location: IVec2Arg::from_str(parcel)?.0,
                        allow_failures,
                    })
                } else {
                    Ok(TestScene {
                        location: IVec2Arg::from_str(scene)?.0,
                        allow_failures: Default::default(),
                    })
                }
            })
            .collect();

        Ok(Self(scenes?))
    }
}

#[derive(Default, Resource)]
pub struct TestingData {
    pub test_mode: bool,
    pub inspect_hash: Option<String>,
    pub test_scenes: Option<TestScenes>,
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(crate) fn initialize_scene(
    mut commands: Commands,
    scene_updates: Res<SceneUpdates>,
    crdt_component_interfaces: Res<CrdtExtractors>,
    mut loading_scenes: Query<(
        Entity,
        &mut SceneLoading,
        &Handle<SceneJsFile>,
        &mut RendererSceneContext,
    )>,
    scene_js_files: Res<Assets<SceneJsFile>>,
    asset_server: Res<AssetServer>,
    ipfs: Res<IpfsResource>,
    wallet: Res<Wallet>,
    testing_data: Res<TestingData>,
) {
    for (root, mut state, h_code, mut context) in loading_scenes.iter_mut() {
        if !matches!(state.as_mut(), SceneLoading::Javascript(_)) || context.tick_number != 1 {
            continue;
        }

        debug!("checking for js");
        let mut fail = |msg: &str| {
            warn!("{root:?} failed to initialize scene: {msg}");
            commands.entity(root).try_insert(SceneLoading::Failed);
        };

        match asset_server.load_state(h_code) {
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
            context.hash.clone(),
            js_file.clone(),
            crdt_component_interfaces,
            thread_sx,
            global_updates,
            ipfs.clone(),
            wallet.clone(),
            scene_id,
            testing_data
                .inspect_hash
                .as_ref()
                .map_or(false, |inspect_hash| inspect_hash == &context.hash),
            testing_data.test_mode,
        );

        // mark context as in flight so we wait for initial RPC requests
        context.in_flight = true;

        commands
            .entity(root)
            .try_insert((SceneThreadHandle { sender: main_sx },));
        commands.entity(root).remove::<SceneLoading>();
    }
}

#[derive(Resource, Default)]
pub struct LiveScenes(pub HashMap<String, Entity>);

pub struct PortableSource {
    pub pid: String,
    pub parent_scene: Option<String>,
    pub ens: Option<String>,
}

#[derive(Resource, Default)]
pub struct PortableScenes(pub HashMap<String, PortableSource>);

pub const PARCEL_SIZE: f32 = 16.0;

#[derive(Resource, Default, Debug)]
pub struct ScenePointers(pub HashMap<IVec2, PointerResult>);

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Clone)]
pub enum PointerResult {
    Nothing {
        realm: String,
        x: i32,
        y: i32,
    },
    Exists {
        realm: String,
        hash: String,
        urn: Option<String>,
    },
}

impl PointerResult {
    fn hash_and_urn(&self) -> Option<(String, Option<String>)> {
        match self {
            PointerResult::Nothing { .. } => None,
            PointerResult::Exists { hash, urn, .. } => Some((hash.clone(), urn.clone())),
        }
    }
}

fn parcels_in_range(focus: &GlobalTransform, range: f32) -> Vec<(IVec2, f32)> {
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
                results.push((parcel, distance));
            }
        }
    }

    results
}

pub fn process_realm_change(
    current_realm: Res<CurrentRealm>,
    // mut pointers: ResMut<ScenePointers>,
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
            let path = match IpfsPath::new_from_urn::<EntityDefinition>(&hacked_urn) {
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

        // pointers.0.retain(|_, pr| match pr {
        //     PointerResult::Nothing{ .. } => false,
        //     PointerResult::Exists{ hash, .. } => realm_scene_ids.contains_key(hash),
        // });
        if !realm_scene_ids.is_empty() {
            // purge pointers and scenes that are not in the realm list
            live_scenes
                .0
                .retain(|hash, _| realm_scene_ids.contains_key(hash));
        }
    }
}

#[allow(clippy::type_complexity)]
fn load_active_entities(
    current_realm: Res<CurrentRealm>,
    focus: Query<&GlobalTransform, With<PrimaryUser>>,
    range: Res<SceneLoadDistance>,
    mut pointers: ResMut<ScenePointers>,
    mut pointer_request: Local<Option<(HashSet<IVec2>, HashMap<String, String>, ActiveEntityTask)>>,
    ipfas: IpfsAssetServer,
) {
    if current_realm.is_changed() {
        // drop current request
        *pointer_request = None;
    }

    if pointer_request.is_none()
        && !current_realm.address.is_empty()
        && ipfas.active_endpoint().is_some()
    {
        let has_scene_urns = !current_realm
            .config
            .scenes_urn
            .as_ref()
            .map_or(true, Vec::is_empty);

        let Ok(focus) = focus.get_single() else {
            return;
        };

        let required_parcels: HashSet<_> = parcels_in_range(focus, range.load)
            .into_iter()
            .filter_map(|(parcel, _)| match pointers.0.get(&parcel) {
                Some(PointerResult::Exists { realm, .. })
                | Some(PointerResult::Nothing { realm, .. }) => {
                    (realm != &current_realm.address).then_some(parcel)
                }
                _ => Some(parcel),
            })
            .collect();

        if !has_scene_urns {
            // load required pointers
            let pointers = required_parcels
                .iter()
                .map(|parcel| format!("{},{}", parcel.x, parcel.y))
                .collect();

            if !required_parcels.is_empty() {
                *pointer_request = Some((
                    required_parcels,
                    HashMap::default(),
                    ipfas
                        .ipfs()
                        .active_entities(ipfs::ActiveEntitiesRequest::Pointers(pointers), None),
                ));
            }
        } else {
            // TODO perf might be worth caching available and required
            let available_hashes = pointers
                .0
                .iter()
                .flat_map(|(_, ptr)| match ptr {
                    PointerResult::Nothing { .. } => None,
                    PointerResult::Exists { realm, hash, .. } => {
                        if realm == &current_realm.address {
                            Some(hash)
                        } else {
                            None
                        }
                    }
                })
                .collect::<HashSet<_>>();

            let required_hashes_and_urns = current_realm
                .config
                .scenes_urn
                .as_ref()
                .unwrap()
                .iter()
                .flat_map(|urn| {
                    IpfsPath::new_from_urn::<EntityDefinition>(urn)
                        .ok()
                        .and_then(|path| {
                            path.context_free_hash()
                                .unwrap_or_default()
                                .map(|hash| (hash, path, urn))
                        })
                })
                .filter(|(hash, ..)| !available_hashes.contains(hash))
                .collect::<Vec<_>>();

            let required_paths = required_hashes_and_urns
                .iter()
                .map(|(_, path, _)| path.clone())
                .collect::<Vec<_>>();
            let lookup = HashMap::from_iter(
                required_hashes_and_urns
                    .into_iter()
                    .map(|(hash, _, urn)| (hash, urn.clone())),
            );

            // issue request if either parcels or urns are non-empty, so that we populate `PointerResult::Nothing`s
            if !required_paths.is_empty() || !required_parcels.is_empty() {
                debug!("requesting {} urns", required_paths.len());
                *pointer_request = Some((
                    required_parcels,
                    lookup,
                    ipfas
                        .ipfs()
                        .active_entities(ipfs::ActiveEntitiesRequest::Urns(required_paths), None),
                ));
            }
        }
    } else if let Some(task_result) = pointer_request.as_mut().and_then(|req| req.2.complete()) {
        // process active scenes in the requested set
        let (mut requested_parcels, mut urn_lookup, _) = pointer_request.take().unwrap();

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
            // TODO check for portables

            let Some(meta) = active_entity
                .metadata
                .and_then(|meta| serde_json::from_value::<SceneMeta>(meta).ok())
            else {
                warn!("active entity scene.json did not resolve to expected format");
                continue;
            };

            let urn = urn_lookup.remove(&active_entity.id);

            for pointer in meta.scene.parcels {
                let (x, y) = pointer.split_once(',').unwrap();
                let x = x.parse::<i32>().unwrap();
                let y = y.parse::<i32>().unwrap();
                let parcel = IVec2::new(x, y);

                requested_parcels.remove(&parcel);
                pointers.0.insert(
                    parcel,
                    PointerResult::Exists {
                        realm: current_realm.address.clone(),
                        hash: active_entity.id.clone(),
                        urn: urn.clone(),
                    },
                );
            }
        }

        // any remaining requested parcels are empty
        for empty_parcel in requested_parcels {
            pointers.0.insert(
                empty_parcel,
                PointerResult::Nothing {
                    realm: current_realm.address.clone(),
                    x: empty_parcel.x,
                    y: empty_parcel.y,
                },
            );
        }
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn process_scene_lifecycle(
    mut commands: Commands,
    current_realm: Res<CurrentRealm>,
    portables: Res<PortableScenes>,
    focus: Query<&GlobalTransform, With<PrimaryUser>>,
    scene_entities: Query<
        (Entity, &SceneHash),
        Or<(With<SceneLoading>, With<RendererSceneContext>)>,
    >,
    range: Res<SceneLoadDistance>,
    mut live_scenes: ResMut<LiveScenes>,
    mut spawn: EventWriter<LoadSceneEvent>,
    pointers: Res<ScenePointers>,
) {
    let mut required_scene_ids: HashSet<(String, Option<String>)> = HashSet::default();

    // add nearby scenes to requirements
    let Ok(focus) = focus.get_single() else {
        return;
    };

    let pir = parcels_in_range(focus, range.load + range.unload);

    required_scene_ids.extend(pir.iter().flat_map(|(parcel, dist)| {
        if *dist < range.load {
            pointers.0.get(parcel).and_then(PointerResult::hash_and_urn)
        } else {
            None
        }
    }));

    // add any portables to requirements
    required_scene_ids.extend(
        portables
            .0
            .iter()
            .map(|(hash, source)| (hash.clone(), Some(source.pid.clone()))),
    );

    // record additional optional scenes
    let mut keep_scene_ids = required_scene_ids.clone();
    keep_scene_ids.extend(pir.iter().flat_map(|(parcel, dist)| {
        if *dist >= range.load {
            pointers.0.get(parcel).and_then(PointerResult::hash_and_urn)
        } else {
            None
        }
    }));

    // record which scene entities we should keep
    let keep_entities: HashMap<_, _> = keep_scene_ids
        .iter()
        .flat_map(|(hash, maybe_urn)| live_scenes.0.get(hash).map(|ent| (ent, (hash, maybe_urn))))
        .collect();

    let mut existing_ids = HashSet::default();
    let mut removed_hashes = Vec::default();

    // despawn any no-longer required entities
    for (entity, scene_hash) in &scene_entities {
        match keep_entities.get(&entity) {
            Some((hash, _)) => {
                existing_ids.insert(<&String>::clone(hash));
            }
            None => {
                if let Some(commands) = commands.get_entity(entity) {
                    info!("despawning {:?}", entity);
                    commands.despawn_recursive();
                }
                removed_hashes.push(&scene_hash.0);
            }
        }
    }
    drop(keep_entities);

    for removed_hash in removed_hashes {
        live_scenes.0.remove(removed_hash);
    }

    // spawn any newly required scenes
    for (required_scene_hash, maybe_urn) in required_scene_ids
        .iter()
        .filter(|(hash, _)| !existing_ids.contains(hash))
    {
        let entity = commands
            .spawn((
                SceneHash(required_scene_hash.clone()),
                SceneLoading::SceneSpawned,
            ))
            .id();
        info!("spawning scene {:?} @ ??: {entity:?}", required_scene_hash);
        live_scenes.0.insert(required_scene_hash.clone(), entity);
        spawn.send(LoadSceneEvent {
            realm: current_realm.address.clone(),
            entity: Some(entity),
            location: match maybe_urn {
                Some(urn) => SceneIpfsLocation::Urn(urn.to_owned()),
                None => SceneIpfsLocation::Hash(required_scene_hash.to_owned()),
            },
        });
    }
}

#[derive(Component)]
pub struct SceneHash(pub String);
