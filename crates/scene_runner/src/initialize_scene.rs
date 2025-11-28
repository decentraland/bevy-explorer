use std::{borrow::Borrow, collections::VecDeque, num::ParseIntError, str::FromStr};

use analytics::segment_system::SegmentConfig;
use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    math::{FloatOrd, Vec3Swizzles},
    pbr::NotShadowCaster,
    platform::collections::{HashMap, HashSet},
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, ShaderRef},
};

use common::{
    structs::{AppConfig, AppError, IVec2Arg, PreviewMode, SceneLoadDistance, SceneMeta},
    util::{TaskExt, TryPushChildrenEx},
};
use comms::global_crdt::GlobalCrdtState;
use dcl::{
    interface::{crdt_context::CrdtContext, CrdtComponentInterfaces, CrdtType},
    SceneElapsedTime, SceneId, SceneResponse,
};
use dcl_component::{
    DclReader, DclWriter, SceneComponentId, SceneEntityId, proto_components::sdk::components::{PbMainCamera, PbRealmInfo}, transform_and_parent::DclTransformAndParent
};
use ipfs::{
    ipfs_path::IpfsPath, ActiveEntityTask, CurrentRealm, EntityDefinition, IpfsAssetServer,
    SceneIpfsLocation, SceneJsFile,
};
use scene_material::BoundRegion;
use system_bridge::{LiveSceneInfo, SystemApi, SystemBridge};

use super::{update_world::CrdtExtractors, LoadSceneEvent, PrimaryUser, SceneSets, SceneUpdates};
use crate::{
    bounds_calc::scene_regions, renderer_context::RendererSceneContext,
    update_world::ComponentTracker, ContainerEntity, DeletedSceneEntities, SceneEntity,
    SceneThreadHandle,
};

#[cfg(not(target_arch = "wasm32"))]
use dcl_deno::spawn_scene;

#[cfg(target_arch = "wasm32")]
use dcl_wasm::spawn_scene;

#[derive(Default)]
pub struct CrdtLoader;

impl AssetLoader for CrdtLoader {
    type Asset = SerializedCrdtStore;
    type Error = std::io::Error;
    type Settings = ();

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &Self::Settings,
        _: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::default();
        reader.read_to_end(&mut bytes).await?;
        Ok(SerializedCrdtStore(bytes))
    }

    fn extensions(&self) -> &[&str] {
        &["crdt"]
    }
}

pub struct SceneLifecyclePlugin;

impl Plugin for SceneLifecyclePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CurrentImposterScene>();
        app.init_resource::<LiveScenes>();
        app.init_resource::<ScenePointers>();
        app.init_resource::<PortableScenes>();
        app.init_asset::<SerializedCrdtStore>();
        app.init_asset_loader::<CrdtLoader>();
        app.add_plugins(MaterialPlugin::<LoadingMaterial>::default());

        app.add_systems(
            Update,
            (
                load_scene_entity,
                load_scene_json,
                load_scene_javascript,
                initialize_scene,
                animate_ready_scene,
                update_loading_quads,
                handle_live_scene_info,
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

#[derive(Component)]
pub struct SceneEntityDefinitionHandle(pub Handle<EntityDefinition>);

#[derive(Component)]
pub struct SceneInitialData {
    pub js: Handle<SceneJsFile>,
}

pub(crate) fn load_scene_entity(
    mut commands: Commands,
    mut load_scene_events: EventReader<LoadSceneEvent>,
    ipfas: IpfsAssetServer,
) {
    for event in load_scene_events.read() {
        let mut commands = match event.entity {
            Some(entity) => {
                let Ok(commands) = commands.get_entity(entity) else {
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
            SceneEntityDefinitionHandle(h_scene),
        ));

        if event.super_user {
            commands.try_insert(SuperUserScene);
        }
    }
}

pub(crate) fn load_scene_json(
    mut commands: Commands,
    mut loading_scenes: Query<(Entity, &mut SceneLoading, &SceneEntityDefinitionHandle)>,
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

        match ipfas.load_state(h_scene.0.id()) {
            bevy::asset::LoadState::Loaded => (),
            bevy::asset::LoadState::Failed(_) => {
                fail("Scene entity could not be loaded");
                continue;
            }
            _ => continue,
        }
        let Some(definition) = scene_definitions.get(h_scene.0.id()) else {
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
    loading_scenes: Query<(Entity, &SceneLoading, &SceneEntityDefinitionHandle)>,
    scene_definitions: Res<Assets<EntityDefinition>>,
    main_crdts: Res<Assets<SerializedCrdtStore>>,
    ipfas: IpfsAssetServer,
    crdt_component_interfaces: Res<CrdtExtractors>,
    mut scene_updates: ResMut<SceneUpdates>,
    global_scene: Res<GlobalCrdtState>,
    portable_scenes: Res<PortableScenes>,
    realm: Res<CurrentRealm>,
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
                bevy::asset::LoadState::Failed(_) => {
                    fail("scene.json could not be loaded");
                    continue;
                }
                _ => continue,
            }
        }

        let Some(definition) = scene_definitions.get(h_scene.0.id()) else {
            fail("definition was dropped");
            continue;
        };
        let Some(raw_meta) = &definition.metadata else {
            fail("definition didn't contain metadata");
            continue;
        };
        let Ok(meta) = serde_json::from_value::<SceneMeta>(raw_meta.clone()) else {
            fail("scene.json did not resolve to expected format");
            continue;
        };

        let portable = portable_scenes.0.get(&definition.id);

        let (base_x, base_y) = meta.scene.base.split_once(',').unwrap();
        let base_x = base_x.parse::<i32>().unwrap();
        let base_y = base_y.parse::<i32>().unwrap();
        let base = IVec2::new(base_x, base_y);

        // populate pointers
        let mut extent_min = IVec2::MAX;
        let mut extent_max = IVec2::MIN;
        let parcels: HashSet<_> = meta
            .scene
            .parcels
            .iter()
            .map(|pointer| {
                let (x, y) = pointer.split_once(',').unwrap();
                let x = x.parse::<i32>().unwrap();
                let y = y.parse::<i32>().unwrap();
                let parcel = IVec2::new(x, y);

                extent_min = extent_min.min(parcel);
                extent_max = extent_max.max(parcel);

                parcel
            })
            .collect();

        let bounds = if portable.is_some() {
            Vec::default()
        } else {
            scene_regions(parcels.clone().into_iter())
                .into_iter()
                .map(|region| BoundRegion::new(region.min, region.max, region.count))
                .collect::<Vec<_>>()
        };

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
                    fail(&format!("couldn't load javascript: {e}"));
                    continue;
                }
            }
        } else {
            ipfas.load_url_uncached(
                "https://renderer-artifacts.decentraland.org/sdk6-adaption-layer/main/index.min.js",
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

        // portable PID, else realm + parcel
        let storage_root = match &portable {
            Some(portable) => portable.pid.clone(),
            None => {
                let about_url = ipfas.ipfs().about_url().unwrap_or_default();
                format!("{about_url}:{}:{}", base.x, base.y)
            }
        };

        info!("{root:?}: started scene (location: {base:?}, scene thread id: {scene_id:?}, is sdk7: {is_sdk7:?}), storage root: {storage_root}");
        let mut renderer_context = RendererSceneContext::new(
            scene_id,
            definition.id.clone(),
            storage_root,
            portable.is_some(),
            title,
            base,
            parcels,
            bounds,
            meta.spawn_points.clone().unwrap_or_default(),
            root,
            1.0,
            config.scene_log_to_console,
            if is_sdk7 { "sdk7" } else { "sdk6" },
            false,
        );

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

        // set initial realm info
        let base_url = realm
            .about_url
            .strip_suffix("/about")
            .unwrap_or(&realm.about_url);
        let realm_info = PbRealmInfo {
            base_url: base_url.to_owned(),
            realm_name: realm.config.realm_name.clone().unwrap_or_default(),
            network_id: realm.config.network_id.unwrap_or_default() as i32,
            comms_adapter: realm
                .comms
                .as_ref()
                .and_then(|comms| comms.adapter.clone())
                .unwrap_or("offline".to_owned()),
            is_preview: false,
            room: None,
            is_connected_scene_room: None,
        };
        buf.clear();
        DclWriter::new(&mut buf).write(&realm_info);
        initial_crdt.force_update(
            SceneComponentId::REALM_INFO,
            CrdtType::LWW_ANY,
            SceneEntityId::ROOT,
            Some(&mut DclReader::new(&buf)),
        );

        if let Some(serialized_crdt) = maybe_serialized_crdt {
            // add main.crdt
            let mut context =
                CrdtContext::new(scene_id, renderer_context.hash.clone(), false, false);
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

        // add MainCamera component
        let mut buf = Vec::default();
        DclWriter::new(&mut buf).write(&PbMainCamera {
            virtual_camera_entity: None,
        });
        initial_crdt.force_update(
            SceneComponentId::MAIN_CAMERA,
            CrdtType::LWW_ENT,
            SceneEntityId::CAMERA,
            Some(&mut DclReader::new(&buf)),
        );

        // store main.crdt + initial global state to post to the scene thread on first request
        renderer_context.crdt_store = initial_crdt;

        commands.entity(root).try_insert((
            Transform::from_translation(Vec3::new(
                initial_position.x,
                -1000.0,
                -initial_position.y,
            )),
            Visibility::default(),
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

        commands.entity(root).try_insert((
            SceneInitialData {
                js: h_code,
            },
            SceneLoading::Javascript(Some(global_updates)),
        ));
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
                println!("parsing test scenes scene {scene}");
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

#[derive(Component)]
pub struct SuperUserScene;

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(crate) fn initialize_scene(
    mut commands: Commands,
    scene_updates: Res<SceneUpdates>,
    crdt_component_interfaces: Res<CrdtExtractors>,
    mut loading_scenes: Query<(
        Entity,
        &mut SceneLoading,
        &SceneInitialData,
        &mut RendererSceneContext,
        Option<&SuperUserScene>,
    )>,
    scene_js_files: Res<Assets<SceneJsFile>>,
    asset_server: Res<AssetServer>,
    testing_data: Res<TestingData>,
    preview_mode: Res<PreviewMode>,
    su_bridge: Res<SystemBridge>,
) {
    for (root, mut state, initial_data, mut context, super_user) in loading_scenes.iter_mut() {
        if !matches!(state.as_mut(), SceneLoading::Javascript(_)) || context.tick_number != 1 {
            continue;
        }

        debug!("checking for js");
        let mut fail = |msg: &str| {
            warn!("{root:?} failed to initialize scene: {msg}");
            commands.entity(root).try_insert(SceneLoading::Failed);
        };

        match asset_server.load_state(initial_data.js.id()) {
            bevy::asset::LoadState::Loaded => (),
            bevy::asset::LoadState::Failed(_) => {
                fail("main js could not be loaded");
                continue;
            }
            _ => continue,
        }

        let Some(js_file) = scene_js_files.get(initial_data.js.id()) else {
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

        let inspected = testing_data
            .inspect_hash
            .as_ref()
            .is_some_and(|inspect_hash| inspect_hash == &context.hash);

        let main_sx = spawn_scene(
            context.crdt_store.clone(),
            context.hash.clone(),
            js_file.clone(),
            crdt_component_interfaces,
            thread_sx,
            global_updates,
            scene_id,
            context.storage_root.clone(),
            inspected,
            testing_data.test_mode,
            preview_mode.is_preview,
            super_user.map(|_| su_bridge.sender.clone()),
        );

        // mark context as in flight so we wait for initial RPC requests
        context.in_flight = true;
        context.inspected = inspected;

        commands
            .entity(root)
            .try_insert((SceneThreadHandle { sender: main_sx },));
        commands.entity(root).remove::<SceneLoading>();
    }
}

#[derive(Resource, Default)]
pub struct LiveScenes {
    pub scenes: HashMap<String, Entity>,
    pub block_new_scenes: bool,
}

pub struct PortableSource {
    pub pid: String,
    pub parent_scene: Option<String>,
    pub ens: Option<String>,
    pub super_user: bool,
}

#[derive(Resource, Default)]
pub struct PortableScenes(pub HashMap<String, PortableSource>);

pub const PARCEL_SIZE: f32 = 16.0;

#[derive(Resource, Debug)]
pub struct ScenePointers {
    pointers: HashMap<IVec2, PointerResult>,
    realm_bounds: (IVec2, IVec2),
    crcs: Vec<Vec<Option<u32>>>,
}

impl Default for ScenePointers {
    fn default() -> Self {
        Self {
            pointers: Default::default(),
            realm_bounds: (IVec2::MAX, IVec2::MIN),
            crcs: Default::default(),
        }
    }
}

impl ScenePointers {
    pub fn is_full(&self) -> bool {
        if self.realm_bounds.0.cmpgt(self.realm_bounds.1).any() {
            return true;
        }

        let expected_count = (self.realm_bounds.1 - self.realm_bounds.0 + 1).element_product();
        self.pointers.len() >= expected_count as usize
    }

    pub fn get(&self, parcel: impl Borrow<IVec2>) -> Option<&PointerResult> {
        let parcel: &IVec2 = parcel.borrow();
        if parcel.cmplt(self.realm_bounds.0).any() || parcel.cmpgt(self.realm_bounds.1).any() {
            return Some(&PointerResult::NOTHING);
        }
        self.pointers.get(parcel)
    }

    pub fn set_realm(&mut self, min_bound: IVec2, max_bound: IVec2) {
        self.realm_bounds = (min_bound, max_bound);
        // clear nothings
        self.pointers.retain(|_, r| r != &PointerResult::Nothing);
        // exists will be rechecked / replaced when active entities returns
        self.crcs.clear();
    }
    pub fn insert(&mut self, parcel: IVec2, result: PointerResult) -> Option<(IVec2, IVec2)> {
        let mut res = None;
        if !matches!(result, PointerResult::Nothing) {
            let new_min = self.realm_bounds.0.min(parcel);
            let new_max = self.realm_bounds.1.max(parcel);

            if (new_min, new_max) != self.realm_bounds {
                res = Some((new_min, new_max));
                self.realm_bounds.0 = new_min;
                self.realm_bounds.1 = new_max;
            }
        }
        self.pointers.insert(parcel, result);
        res
    }

    pub fn min(&self) -> IVec2 {
        self.realm_bounds.0
    }

    pub fn max(&self) -> IVec2 {
        self.realm_bounds.1
    }

    pub fn crc(&mut self, parcel: impl Borrow<IVec2>, level: usize) -> Option<u32> {
        let parcel: IVec2 = *parcel.borrow();

        // println!("crc {parcel} {level}");
        while self.crcs.len() <= level {
            let add_level = self.crcs.len() as u32;
            let bounds = (self.realm_bounds.1 >> add_level) - (self.realm_bounds.0 >> add_level);
            let count = (bounds.x + 1) * (bounds.y + 1);
            self.crcs
                .push(Vec::from_iter(std::iter::repeat_n(None, count as usize)));
            // println!("added {} entry with {} members", self.crcs.len(), self.crcs[self.crcs.len()-1].len());
        }

        let level_bounds_min = self.realm_bounds.0 >> level as u32;
        let level_bounds_max = self.realm_bounds.1 >> level as u32;
        let level_bounds = level_bounds_max - level_bounds_min + 1;
        let level_parcel = parcel >> level as u32;
        if level_parcel.cmplt(level_bounds_min).any() || level_parcel.cmpgt(level_bounds_max).any()
        {
            return Some(0);
        }

        let level_parcel_offset = level_parcel - level_bounds_min;
        let index = (level_parcel_offset.y * level_bounds.x + level_parcel_offset.x) as usize;

        // println!("parcel index {parcel} @ {level} [in {level_bounds} from {level_bounds_min}] = {level_parcel} / {index}");

        if let Some(crc) = self.crcs[level][index] {
            // println!("cached");
            return Some(crc);
        }

        if level == 0 {
            let crc = match self.get(parcel) {
                Some(PointerResult::Exists { hash, .. }) => {
                    crc::Crc::<u32>::new(&crc::CRC_32_CKSUM).checksum(hash.as_bytes())
                }
                Some(PointerResult::Nothing) => 0,
                None => return None,
            };

            // println!("computing level 0");
            self.crcs[level][index] = Some(crc);
            return Some(crc);
        }

        let mut calc = 0;
        // println!("checking sub levels");
        for (ix, offset) in [IVec2::ZERO, IVec2::X, IVec2::Y, IVec2::ONE]
            .into_iter()
            .enumerate()
        {
            if let Some(sub_crc) = self.crc(
                (level_parcel << level as u32) + (offset << (level - 1) as u32),
                level - 1,
            ) {
                calc ^= sub_crc.rotate_right(ix as u32);
            } else {
                // println!("failed {level}");
                return None;
            }
        }
        // println!("success {level}");
        self.crcs[level][index] = Some(calc);
        Some(calc)
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Clone)]
pub enum PointerResult {
    Nothing,
    Exists {
        realm: String,
        hash: String,
        urn: Option<String>,
    },
}

impl PointerResult {
    const NOTHING: Self = Self::Nothing;
}

impl PointerResult {
    pub fn hash_and_urn(&self) -> Option<(String, Option<String>)> {
        match self {
            PointerResult::Nothing => None,
            PointerResult::Exists { hash, urn, .. } => Some((hash.clone(), urn.clone())),
        }
    }

    fn realm(&self) -> Option<&str> {
        match self {
            PointerResult::Nothing => None,
            PointerResult::Exists { realm, .. } => Some(realm),
        }
    }
}

pub fn parcels_in_range(
    focus: &GlobalTransform,
    range: f32,
    min: IVec2,
    max: IVec2,
) -> Vec<(IVec2, f32)> {
    let focus = focus.translation().xz() * Vec2::new(1.0, -1.0);

    let min_point = focus - Vec2::splat(range);
    let max_point = focus + Vec2::splat(range);

    let min_parcel = (min_point / 16.0).floor().as_ivec2().max(min);
    let max_parcel = (max_point / 16.0).ceil().as_ivec2().min(max);

    let mut results = Vec::default();

    for parcel_x in min_parcel.x..=max_parcel.x {
        for parcel_y in min_parcel.y..=max_parcel.y {
            let parcel = IVec2::new(parcel_x, parcel_y);
            let parcel_min_point = parcel.as_vec2() * PARCEL_SIZE;
            let parcel_max_point = (parcel + 1).as_vec2() * PARCEL_SIZE;
            let nearest_point = focus.clamp(parcel_min_point, parcel_max_point);
            let distance = nearest_point.distance(focus);

            if distance <= range && parcel.clamp(min, max) == parcel {
                results.push((parcel, distance));
            }
        }
    }

    results
}

pub fn process_realm_change(
    current_realm: Res<CurrentRealm>,
    mut live_scenes: ResMut<LiveScenes>,
    mut segment_config: Option<ResMut<SegmentConfig>>,
    scenes: Query<&RendererSceneContext>,
) {
    if current_realm.is_changed() {
        info!(
            "realm change `{}` / `{}`! purging scenes",
            current_realm.address, current_realm.about_url
        );
        let mut realm_scene_urns = HashSet::new();
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
            // purge pointers and scenes that are not in the realm list (and not portable)
            live_scenes.scenes.retain(|hash, entity| {
                realm_scene_ids.contains_key(hash)
                    || scenes.get(*entity).is_ok_and(|ctx| ctx.is_portable)
            });
        }

        if let Some(ref mut segment_config) = segment_config {
            segment_config.update_realm(current_realm.address.clone());
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
    mut global_crdt: ResMut<GlobalCrdtState>,
    mut consecutive_fetch_fail_count: Local<usize>,
    mut commands: Commands,
    mut fetch_count: Local<usize>,
    mut stored_parcels: Local<(IVec2, Vec<(f32, IVec2)>)>,
) {
    if current_realm.is_changed() {
        *fetch_count = 100;
        // drop current request
        *pointer_request = None;
        // clear stored parcels
        *stored_parcels = (IVec2::MAX, Vec::default());

        // set current realm and clear
        // take map bounds
        let (mut bounds_min, mut bounds_max) = current_realm
            .config
            .map
            .as_ref()
            .map(|data| data.sizes.iter())
            .unwrap_or_default()
            .fold((IVec2::MAX, IVec2::MIN), |(min, max), region| {
                (
                    min.min(IVec2::new(region.left, region.bottom)),
                    max.max(IVec2::new(region.right, region.top)),
                )
            });
        // take local parcel bounds
        for parcel in current_realm
            .config
            .local_scene_parcels
            .as_ref()
            .map(|p| p.iter())
            .unwrap_or_default()
        {
            if let Ok(IVec2Arg(parcel)) = IVec2Arg::from_str(parcel) {
                bounds_min = bounds_min.min(parcel);
                bounds_max = bounds_max.max(parcel);
            }
        }
        pointers.set_realm(bounds_min, bounds_max);
        global_crdt.set_bounds(bounds_min, bounds_max);
    }

    if pointer_request.is_none()
        && !current_realm.address.is_empty()
        && ipfas.active_endpoint().is_some()
    {
        let has_scene_urns = !current_realm
            .config
            .scenes_urn
            .as_ref()
            .is_none_or(Vec::is_empty);

        let Ok(focus) = focus.single() else {
            return;
        };

        let focus_parcel = (focus.translation().xz() * Vec2::new(1.0 / 16.0, -1.0 / 16.0))
            .floor()
            .as_ivec2();

        if focus_parcel != stored_parcels.0 {
            let mut required_parcels: Vec<_> = parcels_in_range(
                focus,
                range.load.max(range.load_imposter),
                pointers.min(),
                pointers.max(),
            )
            .into_iter()
            .filter_map(|(parcel, distance)| match pointers.get(parcel) {
                Some(PointerResult::Exists { realm, .. }) => {
                    (realm != &current_realm.address).then_some((distance, parcel))
                }
                Some(PointerResult::Nothing) => None,
                _ => Some((distance, parcel)),
            })
            .collect();
            required_parcels.sort_by_key(|(distance, _)| FloatOrd(-distance));
            *stored_parcels = (focus_parcel, required_parcels);
        }

        // limit per request
        let stored_len = stored_parcels.1.len();
        let required_parcels = stored_parcels
            .1
            .split_off(stored_len.saturating_sub(*fetch_count))
            .into_iter()
            .map(|(_, parcel)| parcel)
            .collect::<HashSet<_>>();

        if !has_scene_urns {
            // load required pointers
            let pointers = required_parcels
                .iter()
                .map(|parcel| format!("{},{}", parcel.x, parcel.y))
                .collect::<Vec<_>>();

            if !required_parcels.is_empty() {
                info!("requesting {} parcels", pointers.len());

                *pointer_request = Some((
                    required_parcels,
                    HashMap::default(),
                    ipfas.ipfs().active_entities(
                        ipfs::ActiveEntitiesRequest::Pointers(pointers),
                        current_realm.config.city_loader_content_server.as_deref(),
                    ),
                ));
            }
        } else {
            // TODO perf might be worth caching available and required
            let available_hashes = pointers
                .pointers
                .iter()
                .flat_map(|(_, ptr)| match ptr {
                    PointerResult::Nothing => None,
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
                    ipfas.ipfs().active_entities(
                        ipfs::ActiveEntitiesRequest::Urns(required_paths),
                        current_realm.config.city_loader_content_server.as_deref(),
                    ),
                ));
            }
        }
    } else if let Some(task_result) = pointer_request.as_mut().and_then(|req| req.2.complete()) {
        // process active scenes in the requested set
        let (mut requested_parcels, mut urn_lookup, _) = pointer_request.take().unwrap();

        let retrieved_parcels = match task_result {
            Ok(res) => {
                *consecutive_fetch_fail_count = 0;
                *fetch_count = (*fetch_count * 2).min(3200);
                res
            }
            Err(e) => {
                warn!("failed to retrieve active scenes, will retry");
                warn!("error: {e:?}");
                *fetch_count = (*fetch_count / 2).max(100);
                if *fetch_count == 100 {
                    *consecutive_fetch_fail_count += 1;
                }
                if *consecutive_fetch_fail_count == 10 {
                    warn!("failed to retrieve active scenes 10 times, aborting");
                    commands.send_event(AppError::NetworkFailure(e));
                }
                return;
            }
        };

        info!(
            "found {} entities over {} parcels",
            retrieved_parcels.len(),
            requested_parcels.len(),
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

            let mut urn = urn_lookup.remove(&active_entity.id);

            if urn.is_none() {
                if let Some(scene_server) = current_realm.config.city_loader_content_server.as_ref()
                {
                    // city loader requires we source the scene entity from the city server as well (unless otherwise specified)
                    urn = Some(format!(
                        "urn:decentraland:entity:{}?=&baseUrl={}/contents/",
                        active_entity.id, scene_server
                    ));
                }
            }

            for parcel in meta.scene.parcels.iter().filter_map(|pointer| {
                let (x, y) = pointer.split_once(',').unwrap();
                let x = x.parse::<i32>().ok()?;
                let y = y.parse::<i32>().ok()?;
                Some(IVec2::new(x, y))
            }) {
                requested_parcels.remove(&parcel);
                if let Some(new_bounds) = pointers.insert(
                    parcel,
                    PointerResult::Exists {
                        realm: current_realm.address.clone(),
                        hash: active_entity.id.clone(),
                        urn: urn.clone(),
                    },
                ) {
                    global_crdt.set_bounds(new_bounds.0, new_bounds.1);
                }
            }
        }

        // any remaining requested parcels are empty
        for empty_parcel in requested_parcels {
            pointers
                .pointers
                .insert(empty_parcel, PointerResult::Nothing);
        }
    }
}

#[derive(Resource, Default)]
pub struct CurrentImposterScene(pub Option<(PointerResult, bool)>);

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn process_scene_lifecycle(
    mut commands: Commands,
    current_realm: Res<CurrentRealm>,
    portables: Res<PortableScenes>,
    focus: Query<&GlobalTransform, With<PrimaryUser>>,
    scene_entities: Query<
        (Entity, &SceneHash, Option<&RendererSceneContext>),
        Or<(With<SceneLoading>, With<RendererSceneContext>)>,
    >,
    range: Res<SceneLoadDistance>,
    mut live_scenes: ResMut<LiveScenes>,
    mut spawn: EventWriter<LoadSceneEvent>,
    pointers: Res<ScenePointers>,
    imposter_scene: Res<CurrentImposterScene>,
) {
    let mut required_scene_ids: HashMap<(String, Option<String>), bool> = HashMap::new();

    // add nearby scenes to requirements
    let Ok(focus) = focus.single() else {
        return;
    };

    let current_scene = parcels_in_range(focus, 0.0, pointers.min(), pointers.max())
        .first()
        .and_then(|(p, _)| pointers.get(p))
        .and_then(PointerResult::hash_and_urn);

    let pir = parcels_in_range(
        focus,
        range.load + range.unload,
        pointers.min(),
        pointers.max(),
    );

    required_scene_ids.extend(
        pir.iter()
            .flat_map(|(parcel, dist)| {
                if *dist < range.load {
                    pointers.get(parcel).and_then(PointerResult::hash_and_urn)
                } else {
                    None
                }
            })
            .map(|(h, u)| ((h, u), false)),
    );

    // add any portables to requirements
    required_scene_ids.extend(
        portables
            .0
            .iter()
            .map(|(hash, source)| ((hash.clone(), Some(source.pid.clone())), source.super_user)),
    );

    // add imposter scene
    required_scene_ids.extend(
        imposter_scene
            .0
            .as_ref()
            .and_then(|(scene, _)| scene.hash_and_urn())
            .map(|(h, u)| ((h, u), false)),
    );

    // record additional optional scenes
    let mut keep_scene_ids = required_scene_ids.keys().cloned().collect::<HashSet<_>>();
    keep_scene_ids.extend(pir.iter().flat_map(|(parcel, dist)| {
        if *dist >= range.load && *dist <= range.unload {
            pointers
                .get(parcel)
                // immediately unload scenes from other realms, even if they might match
                // we don't check them until they are in range, so better to just nuke them
                .filter(|pr| pr.realm() == Some(&current_realm.address))
                .and_then(PointerResult::hash_and_urn)
        } else {
            None
        }
    }));

    // record which scene entities we should keep
    let keep_entities: HashMap<_, _> = keep_scene_ids
        .iter()
        .flat_map(|(hash, maybe_urn)| {
            live_scenes
                .scenes
                .get(hash)
                .map(|ent| (ent, (hash, maybe_urn)))
        })
        .collect();

    let mut existing_ids = HashSet::new();
    let mut removed_hashes = Vec::default();

    // despawn any no-longer required entities
    let mut current_scene_loading = false;
    for (entity, scene_hash, maybe_ctx) in &scene_entities {
        match keep_entities.get(&entity) {
            Some((hash, _)) => {
                existing_ids.insert(<&String>::clone(hash));
            }
            None => {
                if let Ok(mut commands) = commands.get_entity(entity) {
                    info!("despawning {:?}", entity);
                    commands.despawn();
                }
                removed_hashes.push(&scene_hash.0);
            }
        }

        // check if the current scene is still loading
        if let Some((current_hash, _)) = current_scene.as_ref() {
            if &scene_hash.0 == current_hash
                && maybe_ctx.is_none_or(|ctx| ctx.tick_number <= 6 && !ctx.broken)
            {
                current_scene_loading = true;
            }
        }
    }
    drop(keep_entities);

    for removed_hash in removed_hashes {
        live_scenes.scenes.remove(removed_hash);
    }

    if let Some(current_scene) = current_scene {
        if required_scene_ids.contains_key(&current_scene)
            && (!existing_ids.contains(&current_scene.0) || current_scene_loading)
        {
            // if the current scene is not even spawned, spawn only that scene
            required_scene_ids.retain(|scene, super_user| *super_user || (scene == &current_scene));
        }
    }

    if live_scenes.block_new_scenes {
        return;
    }

    // spawn any newly required scenes
    for ((required_scene_hash, maybe_urn), super_user) in required_scene_ids
        .iter()
        .filter(|((hash, _), _)| !existing_ids.contains(hash))
    {
        let entity = commands
            .spawn((
                SceneHash(required_scene_hash.clone()),
                SceneLoading::SceneSpawned,
            ))
            .id();
        info!("spawning scene {:?} @ ??: {entity:?}", required_scene_hash);
        live_scenes
            .scenes
            .insert(required_scene_hash.clone(), entity);
        spawn.write(LoadSceneEvent {
            realm: current_realm.address.clone(),
            entity: Some(entity),
            location: match maybe_urn {
                Some(urn) => SceneIpfsLocation::Urn(urn.to_owned()),
                None => SceneIpfsLocation::Hash(required_scene_hash.to_owned()),
            },
            super_user: *super_user,
        });
    }
}

#[derive(Component)]
pub struct SceneHash(pub String);

#[derive(Component)]
pub struct LoadingQuad(bool);

#[allow(clippy::too_many_arguments)]
fn animate_ready_scene(
    mut q: Query<(
        Entity,
        &mut Transform,
        Ref<RendererSceneContext>,
        Option<&Children>,
    )>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut loading_materials: ResMut<Assets<LoadingMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    loading_quads: Query<(), With<LoadingQuad>>,
    preview: Res<PreviewMode>,
    mut handles: Local<Option<(Handle<Mesh>, Handle<StandardMaterial>)>>,
    asset_server: Res<AssetServer>,
    current_imposter_scene: Res<CurrentImposterScene>,
) {
    if handles.is_none() {
        *handles = Some((
            meshes.add(
                Rectangle::default()
                    .mesh()
                    .build()
                    .scaled_by(Vec3::splat(PARCEL_SIZE)),
            ),
            materials.add(StandardMaterial {
                base_color_texture: Some(asset_server.load("embedded://images/grid.png")),
                ..Default::default()
            }),
        ));
    }

    for (root, mut transform, ctx, children) in q.iter_mut() {
        // skip animating imposters
        if current_imposter_scene
            .0
            .as_ref()
            .and_then(|(scene, _)| scene.hash_and_urn())
            .is_some_and(|(hash, _)| hash == ctx.hash)
        {
            continue;
        }

        if transform.translation.y < 0.0 && (ctx.tick_number >= 5 || ctx.broken) {
            if transform.translation.y == -1000.0 {
                for child in children.map(|c| c.iter()).unwrap_or_default() {
                    if loading_quads.get(child).is_ok() {
                        commands.entity(child).despawn();
                    }
                }
            }

            // transform.translation.y *= 0.75;
            // if transform.translation.y > -0.01 {
            transform.translation.y = 0.0;
            // }
        }

        if ctx.is_added() {
            let mut children = Vec::new();
            for parcel in ctx.parcels.iter() {
                let position = ((*parcel - ctx.base) * IVec2::new(1, -1))
                    .as_vec2()
                    .extend(0.0)
                    .xzy()
                    * PARCEL_SIZE;
                let middle = Vec3::new(PARCEL_SIZE * 0.5, 0.0, PARCEL_SIZE * -0.5);

                for (parcel_offset, position_offset, is_x) in [
                    (-IVec2::X, -Vec3::Z * 0.5, true),
                    (IVec2::X, -Vec3::Z * 0.5 + Vec3::X, true),
                    (-IVec2::Y, Vec3::X * 0.5, false),
                    (IVec2::Y, Vec3::X * 0.5 - Vec3::Z, false),
                ] {
                    if !ctx.parcels.contains(&(*parcel + parcel_offset)) {
                        children.push(
                            commands
                                .spawn((
                                    Mesh3d(handles.as_ref().unwrap().0.clone()),
                                    MeshMaterial3d(
                                        loading_materials.add(LoadingMaterial::default()),
                                    ),
                                    Transform::from_translation(
                                        position + position_offset * PARCEL_SIZE + Vec3::Y * 1000.0,
                                    )
                                    .looking_at(position + middle + Vec3::Y * 1000.0, Vec3::Y),
                                    LoadingQuad(is_x),
                                    NotShadowCaster,
                                ))
                                .id(),
                        );
                    }
                }

                if preview.is_preview {
                    children.push(
                        commands
                            .spawn((
                                Mesh3d(handles.as_ref().unwrap().0.clone()),
                                MeshMaterial3d(handles.as_ref().unwrap().1.clone()),
                                Transform::from_translation(
                                    position
                                        + Vec3::new(PARCEL_SIZE * 0.5, -0.01, PARCEL_SIZE * -0.5),
                                )
                                .looking_at(
                                    position
                                        + Vec3::new(PARCEL_SIZE * 0.5, -2.0, -PARCEL_SIZE * 0.5),
                                    Vec3::Z,
                                ),
                            ))
                            .id(),
                    );
                }
            }

            commands.entity(root).try_push_children(&children);
        }
    }
}

#[derive(Component)]
pub struct LoadingMaterialHandle(pub Handle<LoadingMaterial>);

fn update_loading_quads(
    mut q: Query<
        (
            &GlobalTransform,
            &mut Transform,
            &MeshMaterial3d<LoadingMaterial>,
            &LoadingQuad,
        ),
        Without<PrimaryUser>,
    >,
    player: Query<&Transform, With<PrimaryUser>>,
    mut mats: ResMut<Assets<LoadingMaterial>>,
    mut local_prev_active: Local<HashSet<AssetId<LoadingMaterial>>>,
) {
    let Ok(player_translation) = player.single().map(|p| p.translation) else {
        return;
    };

    let prev_active = std::mem::take(&mut *local_prev_active);

    for (gt, mut trans, h_mat, loading) in q.iter_mut() {
        let nearest_point = player_translation.xz().clamp(
            gt.translation().xz()
                - if loading.0 {
                    Vec2::Y * PARCEL_SIZE * 0.5
                } else {
                    Vec2::X * PARCEL_SIZE * 0.5
                },
            gt.translation().xz()
                + if loading.0 {
                    Vec2::Y * PARCEL_SIZE * 0.5
                } else {
                    Vec2::X * PARCEL_SIZE * 0.5
                },
        );
        let active = (nearest_point - player_translation.xz()).length() < 10.0;
        if prev_active.contains(&h_mat.0.id()) || active {
            let mat = mats.get_mut(h_mat.0.id()).unwrap();
            mat.player_pos = player_translation.extend(if active { 1.0 } else { 0.0 })
        }

        trans.translation.y = player_translation.y + 1000.0;

        if active {
            local_prev_active.insert(h_mat.0.id());
        }
    }
}

#[derive(Asset, TypePath, Clone, AsBindGroup, Default)]
pub struct LoadingMaterial {
    #[uniform(0)]
    player_pos: Vec4, // xyz = player pos, w = active
}

impl Material for LoadingMaterial {
    fn fragment_shader() -> bevy::render::render_resource::ShaderRef {
        ShaderRef::Path("embedded://shaders/loading.wgsl".into())
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
}

pub fn handle_live_scene_info(
    mut events: EventReader<SystemApi>,
    scenes: Query<(&RendererSceneContext, Option<&SuperUserScene>)>,
    ipfas: IpfsAssetServer,
) {
    let mut senders = events
        .read()
        .filter_map(|ev| {
            if let SystemApi::LiveSceneInfo(sender) = ev {
                Some(sender)
            } else {
                None
            }
        })
        .peekable();

    if senders.peek().is_none() {
        return;
    }

    let base_urls = ipfas.ipfs().base_urls();

    let scene_info = scenes
        .iter()
        .map(|(ctx, maybe_super)| LiveSceneInfo {
            hash: ctx.hash.clone(),
            base_url: base_urls.get(&ctx.hash).map(ToOwned::to_owned),
            title: ctx.title.clone(),
            parcels: ctx
                .parcels
                .iter()
                .map(|v| dcl_component::proto_components::common::Vector2::from(v.as_vec2()))
                .collect(),
            is_portable: ctx.is_portable,
            is_broken: ctx.broken,
            is_blocked: !ctx.blocked.is_empty(),
            is_super: maybe_super.is_some(),
            sdk_version: ctx.sdk_version.to_string(),
        })
        .collect::<Vec<_>>();

    for sender in senders {
        sender.send(scene_info.clone());
    }
}
