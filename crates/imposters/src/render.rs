use std::path::PathBuf;

use bevy::{
    diagnostic::FrameCount,
    ecs::system::SystemParam,
    math::FloatOrd,
    pbr::{NotShadowCaster, NotShadowReceiver},
    platform::collections::{hash_map::Entry, HashMap, HashSet},
    prelude::*,
    render::{
        mesh::{MeshAabb, VertexAttributeValues},
        primitives::Aabb,
        view::{NoFrustumCulling, RenderLayers},
    },
    tasks::{IoTaskPool, Task},
};
use boimp::{bake::ImposterBakeMaterialPlugin, render::Imposter, ImposterLoaderSettings};
use common::{
    sets::SceneSets,
    structs::{AppConfig, PrimaryCamera, PrimaryUser},
    util::{TaskCompat, TaskExt},
};
use ipfs::{CurrentRealm, IpfsAssetServer};

use scene_runner::{
    initialize_scene::{
        CurrentImposterScene, LiveScenes, PointerResult, SceneLoading, ScenePointers, PARCEL_SIZE,
    },
    renderer_context::RendererSceneContext,
    DebugInfo,
};

use crate::{
    bake_scene::IMPOSTERCEPTION_LAYER,
    floor_imposter::{FloorImposter, FloorImposterLoader},
    imposter_mesh::ImposterMesh,
    imposter_spec::{floor_path, load_imposter, texture_path, BakedScene, ImposterSpec},
    DclImposterPlugin,
};

pub struct DclImposterRenderPlugin;

impl Plugin for DclImposterRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            MaterialPlugin::<FloorImposter>::default(),
            ImposterBakeMaterialPlugin::<FloorImposter>::default(),
        ))
        .init_resource::<BakingIngredients>()
        .init_resource::<ImposterFocus>()
        .init_resource::<ImposterManagerData>()
        .init_asset_loader::<FloorImposterLoader>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (|mut manager: ImposterSpecManager| manager.start_tick()).in_set(SceneSets::Init),
        )
        .add_systems(
            Update,
            (
                focus_imposters,
                spawn_imposters,
                load_imposters,
                debug_write_imposters,
            )
                .chain()
                .in_set(SceneSets::PostLoop),
        )
        .add_systems(
            Update,
            (|mut manager: ImposterSpecManager| manager.end_tick())
                .in_set(SceneSets::RestrictedActions),
        );
    }
}

#[derive(Resource)]
struct ImposterMeshes {
    cube: Handle<Mesh>,
    aabb: Aabb,
    floor: Handle<Mesh>,
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    let mut floor = Plane3d {
        normal: Dir3::Y,
        half_size: Vec2::splat(0.5),
    }
    .mesh()
    .build();

    let Some(VertexAttributeValues::Float32x2(uvs)) = floor.attribute_mut(Mesh::ATTRIBUTE_UV_0)
    else {
        panic!()
    };
    for uv in uvs.iter_mut() {
        uv[0] = 0.0 / 18.0 + 17.0 / 18.0 * uv[0];
        uv[1] = 0.0 / 18.0 + 17.0 / 18.0 * uv[1];
    }

    let cube: Mesh = ImposterMesh::default().build();
    let aabb = cube.compute_aabb().unwrap();

    debug!("base mesh: {:#?}", cube);

    commands.insert_resource(ImposterMeshes {
        cube: meshes.add(cube),
        aabb,
        floor: meshes.add(floor),
    });
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SceneImposter {
    pub parcel: IVec2,
    pub level: usize,
    pub as_ingredient: bool,
}

#[derive(Component)]
pub struct ImposterLoadTask(Task<Option<BakedScene>>);

impl ImposterLoadTask {
    pub fn new_scene(ipfas: &IpfsAssetServer, scene_hash: &str, download: bool) -> Self {
        Self(IoTaskPool::get().spawn_compat(load_imposter(
            ipfas.ipfs().clone(),
            scene_hash.to_string(),
            IVec2::MAX,
            0,
            None, // don't need to check since we load by id,
            download,
        )))
    }

    pub fn new_mip(
        ipfas: &IpfsAssetServer,
        address: &str,
        parcel: IVec2,
        level: usize,
        crc: u32,
        download: bool,
    ) -> Self {
        Self(IoTaskPool::get().spawn_compat(load_imposter(
            ipfas.ipfs().clone(),
            address.to_string(),
            parcel,
            level,
            Some(crc),
            download,
        )))
    }
}

#[derive(Resource, Default)]
pub struct BakingIngredients(pub Vec<(IVec2, usize)>);

#[derive(Resource, Default, Debug)]
pub struct ImposterFocus {
    pub origin: Vec2,
    pub min_distance: f32,
    pub distance_scale: f32,
}

pub fn focus_imposters(
    focus_player: Query<&GlobalTransform, With<PrimaryUser>>,
    focus_camera: Query<&GlobalTransform, With<PrimaryCamera>>,
    mut focus: ResMut<ImposterFocus>,
) {
    // check if looking at player
    let Some((_, cam_rot, cam_pos)) = focus_camera
        .single()
        .ok()
        .map(|gt| gt.to_scale_rotation_translation())
    else {
        return;
    };
    let Some(player_pos) = focus_player.single().ok().map(|gt| gt.translation()) else {
        return;
    };
    let focus_player = (cam_pos - player_pos).length() < 100.0;

    let origin = match focus_player {
        true => player_pos,
        false => {
            let cam_fwd = cam_rot * Vec3::NEG_Z;
            cam_pos - cam_fwd * cam_pos.y / cam_fwd.y
            // debug!("player pos: {player_pos}, ground_intersect: {ground_intersect}");
        }
    };

    focus.origin = origin.xz();
    (focus.min_distance, focus.distance_scale) = if focus_player {
        (0.0, 1.0)
    } else {
        (cam_pos.distance(origin) * 0.5, 0.5)
    };
    debug!("focus: {focus:?}");
}

pub fn spawn_imposters(
    mut commands: Commands,
    config: Res<AppConfig>,
    focus: Res<ImposterFocus>,
    mut required: Local<HashMap<(IVec2, usize), bool>>,
    current_imposters: Query<(Entity, &SceneImposter)>,
    current_realm: Res<CurrentRealm>,
    ingredients: Res<BakingIngredients>,
    live_scenes: Res<LiveScenes>,
    scenes: Query<&RendererSceneContext, Without<SceneLoading>>,
    current_imposter_scene: Res<CurrentImposterScene>,
    mut manager: ImposterSpecManager,
) {
    if current_realm.is_changed() || config.scene_imposter_distances.is_empty() {
        for (entity, _) in &current_imposters {
            if let Ok(mut commands) = commands.get_entity(entity) {
                commands.despawn();
            }
        }

        manager.clear();

        debug!("purge");
        return;
    }

    // skip if no realm
    if manager.pointers.min() == IVec2::MAX {
        return;
    }

    // add baking requirements
    required.extend(ingredients.0.iter().map(|(p, l)| ((*p, *l), true)));

    let origin = focus.origin * Vec2::new(1.0, -1.0);

    // record live parcels
    let current_imposter_scene = match &current_imposter_scene.0 {
        Some((PointerResult::Exists { hash, .. }, _)) => Some(hash),
        _ => None,
    };
    let live_parcels = live_scenes
        .scenes
        .iter()
        .filter(|(hash, _)| Some(*hash) != current_imposter_scene)
        .flat_map(|(_, e)| {
            let Ok(ctx) = scenes.get(*e) else {
                return None;
            };
            if ctx.is_portable {
                return None;
            }
            if !ctx.broken && (ctx.tick_number <= 5 || !ctx.blocked.is_empty()) {
                // not ready
                return None;
            }
            Some(&ctx.parcels)
        })
        .flatten()
        .copied()
        .collect::<HashSet<_>>();
    let live_min = live_parcels.iter().fold(IVec2::MAX, |x, y| x.min(*y));
    let live_max = live_parcels.iter().fold(IVec2::MIN, |x, y| x.max(*y));

    let max_distance = config
        .scene_imposter_distances
        .last()
        .copied()
        .unwrap_or(0.0);
    let mut level = config.scene_imposter_distances.len() - 1;

    let tile_size = 1 << level;

    let min_tile = ((origin - max_distance) / 16.0 / tile_size as f32)
        .floor()
        .as_ivec2();
    let max_tile = ((origin + max_distance) / 16.0 / tile_size as f32)
        .ceil()
        .as_ivec2();

    let min_tile = min_tile.max(manager.pointers.min() >> level as u32);
    let max_tile = max_tile.min(manager.pointers.max() >> level as u32);

    let mut required_tiles = (min_tile.x..=max_tile.x)
        .flat_map(|x| (min_tile.y..=max_tile.y).map(move |y| IVec2::new(x, y)))
        .collect::<HashSet<_>>();

    // take the largest permitted tile to fill the area
    while level > 0 {
        let tile_size = 1 << level;
        let tile_size_world = (tile_size * 16) as f32;

        for tile in std::mem::take(&mut required_tiles).into_iter() {
            let tile_origin_parcel = tile * tile_size;
            let tile_origin_world = tile_origin_parcel.as_vec2() * 16.0;

            let closest_point =
                origin.clamp(tile_origin_world, tile_origin_world + tile_size_world);
            let closest_distance =
                (closest_point - origin).length() * focus.distance_scale + focus.min_distance;

            // check it's not too far
            if closest_distance > max_distance {
                continue;
            }

            let mut render_tile = true;
            // check it's not too close
            render_tile &= closest_distance >= config.scene_imposter_distances[level - 1];
            // ensure no live scenes intersect the tile
            render_tile &= {
                live_max.cmplt(tile_origin_parcel).any()
                    || live_min.cmpge(tile_origin_parcel + tile_size).any()
                    || live_parcels.iter().all(|p| {
                        p.cmplt(tile_origin_parcel).any()
                            || p.cmpge(tile_origin_parcel + tile_size).any()
                    })
            };

            if render_tile {
                trace!("adding {}:{} == {}", tile, level, tile_origin_parcel);
                required.entry((tile_origin_parcel, level)).or_default();
            } else {
                // add to next level requirements
                trace!(
                    "cant' add {}:{} == {} (distance = {} vs {}, live minmax = {},{})",
                    tile,
                    level,
                    tile_origin_parcel,
                    closest_distance,
                    config.scene_imposter_distances[level - 1],
                    live_min,
                    live_max
                );
                for offset in [IVec2::ZERO, IVec2::X, IVec2::Y, IVec2::ONE] {
                    trace!("maybe the child {}:{}", tile * 2 + offset, level - 1);
                    required_tiles.insert(tile * 2 + offset);
                }
            }
        }

        level -= 1;
    }

    for remaining_parcel in required_tiles {
        if !live_parcels.contains(&remaining_parcel) {
            required.entry((remaining_parcel, 0)).or_default();
        }
    }

    // remove old
    let mut old = HashMap::new();
    for (ent, scene_imposter) in current_imposters.iter() {
        let required_as_ingredient =
            required.remove(&(scene_imposter.parcel, scene_imposter.level));

        match required_as_ingredient {
            None => {
                commands.entity(ent).remove::<SceneImposter>();
                old.insert(scene_imposter, ent);
                debug!("remove {:?}", scene_imposter);
            }
            Some(as_ingredient) => {
                if scene_imposter.as_ingredient != as_ingredient {
                    commands.entity(ent).remove::<SceneImposter>();
                    old.insert(scene_imposter, ent);
                    required.insert((scene_imposter.parcel, scene_imposter.level), as_ingredient);
                }
            }
        }
    }

    // add new
    for ((parcel, level), as_ingredient) in required.drain() {
        let scene_imposter = SceneImposter {
            parcel,
            level,
            as_ingredient,
        };

        let max_parcel = parcel + (1 << level);
        old.retain(|k, v| {
            let is_child =
                k.parcel.cmpge(parcel).all() && (k.parcel + (1 << k.level)).cmple(max_parcel).all();
            if is_child {
                manager.store_removed(Some(scene_imposter), *v);
            }
            !is_child
        });

        commands.spawn((Transform::default(), Visibility::default(), scene_imposter));

        debug!("require {}: {} [{}]", level, parcel, as_ingredient);
    }

    // remove any not required non-children
    for (_, ent) in old.drain() {
        manager.store_removed(None, ent);
    }
}

#[derive(Debug)]
pub enum ImposterSpecResolveState {
    Missing,
    PendingRemote,
    Ready(BakedScene),
}

pub enum ImposterSpecLoadState {
    Local(ImposterLoadTask),
    PendingRemote(f32),
    Remote(ImposterLoadTask),
}

pub struct AssetLoadRequest {
    spec: SpecStateReady,
    benefit: f32,
    #[cfg(debug_assertions)]
    debug: Vec<(SceneImposter, Option<usize>, f32, usize)>,
}

impl AssetLoadRequest {
    pub fn new(spec: SpecStateReady) -> Self {
        Self {
            spec,
            benefit: 0.0,
            #[cfg(debug_assertions)]
            debug: Vec::default(),
        }
    }
}

#[derive(Resource, Default)]
pub struct ImposterManagerData {
    pub mips: HashMap<(IVec2, usize, u32), ImposterSpecResolveState>,
    pub scenes: HashMap<String, ImposterSpecResolveState>,

    pub loading_mips: HashMap<(IVec2, usize, u32), ImposterSpecLoadState>,
    pub loading_scenes: HashMap<String, ImposterSpecLoadState>,

    pub prev_loading_mips: HashMap<(IVec2, usize, u32), ImposterSpecLoadState>,
    pub prev_loading_scenes: HashMap<String, ImposterSpecLoadState>,

    pub requested_loading_handles: HashMap<SceneImposter, AssetLoadRequest>,
    pub loading_handles: HashMap<SceneImposter, (Option<UntypedHandle>, Option<UntypedHandle>)>,

    pub just_removed: HashMap<Option<SceneImposter>, Vec<Entity>>,
}

#[derive(PartialEq, Debug)]
pub struct SpecStateReady {
    pub imposter_data: Option<(ImposterSpec, PathBuf)>,
    pub floor_data: Option<PathBuf>,
}

#[derive(PartialEq, Debug)]
pub enum ImposterSpecState {
    Ready(SpecStateReady),
    Pending,
    Missing,
}

pub type LevelError = usize;

#[derive(Debug)]
pub enum ImposterState {
    Ready(
        Option<(Handle<Imposter>, ImposterSpec)>,
        Option<Handle<FloorImposter>>,
    ),
    PendingWithSubstitute(
        SceneImposter,
        Option<(Handle<Imposter>, ImposterSpec)>,
        Option<Handle<FloorImposter>>,
        LevelError,
    ),
    PendingWithPrevious(Vec<Entity>, LevelError),
    Pending(LevelError),
    Missing,
}

#[derive(SystemParam)]
pub struct ImposterSpecManager<'w, 's> {
    commands: Commands<'w, 's>,
    data: ResMut<'w, ImposterManagerData>,
    current_realm: Res<'w, CurrentRealm>,
    pub(crate) pointers: ResMut<'w, ScenePointers>,
    ipfas: IpfsAssetServer<'w, 's>,
    focus: Res<'w, ImposterFocus>,
    plugin: Res<'w, DclImposterPlugin>,
}

const MAX_ASSET_LOADS: usize = 20;
const MAX_ASSET_DOWNLOADS: usize = 10;

impl<'w, 's> ImposterSpecManager<'w, 's> {
    fn clear(&mut self) {
        self.data.mips.clear();
        self.data.loading_mips.clear();
        self.data.prev_loading_mips.clear();
    }

    pub(crate) fn clear_scene(&mut self, hash: &str) {
        self.data.scenes.remove(hash);
    }

    pub(crate) fn clear_mip(&mut self, parcel: IVec2, level: usize, crc: u32) {
        self.data.mips.remove(&(parcel, level, crc));
    }

    fn start_tick(&mut self) {
        self.data.prev_loading_mips = std::mem::take(&mut self.data.loading_mips);
        self.data.prev_loading_scenes = std::mem::take(&mut self.data.loading_scenes);
    }

    fn store_removed(&mut self, child_of: Option<SceneImposter>, entity: Entity) {
        self.data
            .just_removed
            .entry(child_of)
            .or_default()
            .push(entity);
    }

    pub fn get_spec(&mut self, req: &SceneImposter, benefit: f32) -> ImposterSpecState {
        let ImposterManagerData {
            mips,
            scenes,
            loading_mips,
            loading_scenes,
            prev_loading_mips,
            prev_loading_scenes,
            ..
        } = &mut *self.data;

        let resolve = |spec: Option<&ImposterSpecResolveState>,
                       path: &str,
                       has_floor: bool|
         -> ImposterSpecState {
            match spec {
                Some(ImposterSpecResolveState::Missing) => ImposterSpecState::Missing,
                Some(ImposterSpecResolveState::PendingRemote) | None => ImposterSpecState::Pending,
                Some(ImposterSpecResolveState::Ready(baked_scene)) => {
                    let spec = baked_scene.imposters.get(&req.parcel).copied();
                    let imposter_data = spec.map(|s| {
                        (
                            s,
                            texture_path(self.ipfas.ipfs_cache_path(), path, req.parcel, req.level),
                        )
                    });
                    let floor_data = has_floor.then(|| {
                        floor_path(self.ipfas.ipfs_cache_path(), path, req.parcel, req.level)
                    });

                    ImposterSpecState::Ready(SpecStateReady {
                        imposter_data,
                        floor_data,
                    })
                }
            }
        };

        if req.level == 0 {
            let hash = match self.pointers.get(req.parcel) {
                None => return ImposterSpecState::Pending,
                Some(PointerResult::Nothing) => return ImposterSpecState::Missing,
                Some(PointerResult::Exists { hash, .. }) => hash,
            };

            let resolve_state = scenes.get(hash);
            let state = resolve(resolve_state, hash, true);
            if state != ImposterSpecState::Pending {
                return state;
            }

            // manage existing state from previous frame
            if let Some((hash, mut load_state)) = prev_loading_scenes.remove_entry(hash) {
                match load_state {
                    ImposterSpecLoadState::Local(ref mut imposter_load_task) => {
                        // check if local load completed successfully
                        if let Some(maybe_baked_scene) = imposter_load_task.0.complete() {
                            let new_state = match maybe_baked_scene {
                                Some(baked_scene) => ImposterSpecResolveState::Ready(baked_scene),
                                None => ImposterSpecResolveState::PendingRemote,
                            };
                            debug!("end local fetch {hash}: {new_state:?}");
                            scenes.insert(hash, new_state);
                        } else {
                            // reinsert for this frame if not
                            loading_scenes.insert(hash, load_state);
                        }
                    }
                    ImposterSpecLoadState::Remote(ref mut imposter_load_task) => {
                        // check if remote load completed successfully
                        if let Some(maybe_baked_scene) = imposter_load_task.0.complete() {
                            let new_state = match maybe_baked_scene {
                                Some(baked_scene) => ImposterSpecResolveState::Ready(baked_scene),
                                None => ImposterSpecResolveState::Missing,
                            };
                            scenes.insert(hash, new_state);
                        } else {
                            // reinsert for this frame if not
                            loading_scenes.insert(hash, load_state);
                        }
                    }
                    // reset the benefit for this frame
                    ImposterSpecLoadState::PendingRemote(_) => {
                        loading_scenes.insert(hash, ImposterSpecLoadState::PendingRemote(benefit));
                    }
                }
            } else {
                match loading_scenes.entry(hash.clone()) {
                    Entry::Vacant(v) => {
                        // no existing entry, create a new local task or remote request
                        match resolve_state {
                            None => {
                                debug!("start local fetch {hash}");
                                v.insert(ImposterSpecLoadState::Local(
                                    ImposterLoadTask::new_scene(&self.ipfas, hash, false),
                                ));
                            }
                            Some(ImposterSpecResolveState::PendingRemote) => {
                                debug!("requesting remote fetch {hash} (via {req:?})");
                                v.insert(ImposterSpecLoadState::PendingRemote(benefit));
                            }
                            // we should never reach here if the imposter is missing or ready
                            _ => panic!(),
                        }
                    }
                    Entry::Occupied(mut o) => {
                        if let ImposterSpecLoadState::PendingRemote(ref mut prev_benefit) =
                            o.get_mut()
                        {
                            // add our benefit to the total for this remote request
                            *prev_benefit += benefit
                        }
                    }
                }
            }

            resolve(scenes.get(hash), hash, true)
        } else {
            let Some(crc) = self.pointers.crc(req.parcel, req.level) else {
                return ImposterSpecState::Pending;
            };

            let key = (req.parcel, req.level, crc);

            let resolve_state = mips.get(&key);
            let state = resolve(resolve_state, &self.current_realm.about_url, crc != 0);
            if state != ImposterSpecState::Pending {
                return state;
            }

            if let Some(mut load_state) = prev_loading_mips.remove(&key) {
                match load_state {
                    ImposterSpecLoadState::Local(ref mut imposter_load_task) => {
                        if let Some(maybe_baked_scene) = imposter_load_task.0.complete() {
                            let new_state = match maybe_baked_scene {
                                Some(baked_scene) => ImposterSpecResolveState::Ready(baked_scene),
                                None => ImposterSpecResolveState::PendingRemote,
                            };
                            mips.insert(key, new_state);
                        } else {
                            loading_mips.insert(key, load_state);
                        }
                    }
                    ImposterSpecLoadState::Remote(ref mut imposter_load_task) => {
                        if let Some(maybe_baked_scene) = imposter_load_task.0.complete() {
                            let new_state = match maybe_baked_scene {
                                Some(baked_scene) => ImposterSpecResolveState::Ready(baked_scene),
                                None => ImposterSpecResolveState::Missing,
                            };
                            mips.insert(key, new_state);
                        } else {
                            loading_mips.insert(key, load_state);
                        }
                    }
                    ImposterSpecLoadState::PendingRemote(_) => {
                        loading_mips.insert(key, ImposterSpecLoadState::PendingRemote(benefit));
                    }
                }
            } else {
                match loading_mips.entry(key) {
                    Entry::Vacant(v) => match resolve_state {
                        None => {
                            debug!("start local fetch {req:?}");
                            v.insert(ImposterSpecLoadState::Local(ImposterLoadTask::new_mip(
                                &self.ipfas,
                                &self.current_realm.about_url,
                                req.parcel,
                                req.level,
                                crc,
                                false,
                            )));
                        }
                        Some(ImposterSpecResolveState::PendingRemote) => {
                            debug!("requesting remote fetch {req:?}");
                            v.insert(ImposterSpecLoadState::PendingRemote(benefit));
                        }
                        _ => panic!(),
                    },
                    Entry::Occupied(mut o) => {
                        if let ImposterSpecLoadState::PendingRemote(ref mut prev_benefit) =
                            o.get_mut()
                        {
                            *prev_benefit += benefit
                        }
                    }
                }
            }

            resolve(mips.get(&key), &self.current_realm.about_url, crc != 0)
        }
    }

    pub fn get_imposter(
        &mut self,
        req: &SceneImposter,
        current_error: Option<LevelError>,
    ) -> ImposterState {
        let parcel_count = (1 << req.level) * (1 << req.level);

        let origin = self.focus.origin.as_ivec2() * IVec2::new(1, -1);
        let closest_point = origin.clamp(req.parcel * 16, (req.parcel + (1 << req.level)) * 16);
        let distance = (closest_point - origin).as_vec2().length() * self.focus.distance_scale
            + self.focus.min_distance
            + 1.0;
        let parcel_benefit = current_error.unwrap_or(0);
        let benefit = parcel_benefit as f32 / (distance * distance);

        let spec_state = self.get_spec(req, benefit);

        match spec_state {
            ImposterSpecState::Pending => (),
            ImposterSpecState::Missing => return ImposterState::Missing,
            ImposterSpecState::Ready(ready) => {
                let ImposterManagerData {
                    requested_loading_handles,
                    ..
                } = &mut *self.data;

                // request loading if required
                let mut needs_load = false;
                let imposter_data = ready.imposter_data.as_ref().and_then(|(spec, path)| {
                    let Some(handle) = self.ipfas.asset_server().get_handle(path.as_path()) else {
                        needs_load = true;
                        return None;
                    };
                    Some((handle, *spec))
                });

                let floor_data = ready.floor_data.as_ref().and_then(|path| {
                    let Some(handle) = self.ipfas.asset_server().get_handle(path.as_path()) else {
                        needs_load = true;
                        return None;
                    };
                    Some(handle)
                });

                needs_load |= imposter_data.as_ref().is_some_and(|(handle, _)| {
                    !self
                        .ipfas
                        .asset_server()
                        .is_loaded_with_dependencies(handle.id())
                });

                needs_load |= floor_data.as_ref().is_some_and(|handle| {
                    !self
                        .ipfas
                        .asset_server()
                        .is_loaded_with_dependencies(handle.id())
                });

                if needs_load {
                    debug!("req {req:?}");
                    let load_request = requested_loading_handles
                        .entry(*req)
                        .or_insert_with(|| AssetLoadRequest::new(ready));
                    load_request.benefit += benefit;
                    #[cfg(debug_assertions)]
                    load_request
                        .debug
                        .push((*req, current_error, distance, parcel_benefit));
                } else {
                    return ImposterState::Ready(imposter_data, floor_data);
                }
            }
        };

        // not downloaded or not spawned, fallback to best available
        if req.as_ingredient {
            return ImposterState::Pending(0);
        }

        // check for previously despawned lower mip entities first
        if let Some(entities) = self.data.just_removed.remove(&Some(*req)) {
            return ImposterState::PendingWithPrevious(entities, 1);
        }

        // check for larger mips
        for level in req.level + 1..=5 {
            let parcel_err = level - req.level;
            let new_error = parcel_err * parcel_count;
            if let Some(ce) = current_error {
                if ce <= new_error {
                    return ImposterState::Pending(ce);
                }
            }
            let parcel_benefit = current_error.map(|ce| ce - new_error).unwrap_or(0);
            let benefit = parcel_benefit as f32 / (distance * distance);

            // use mask to knock out the lower bits, this works for negative numbers too
            let origin = req.parcel & !((1 << level) - 1);

            let substitute_imposter = SceneImposter {
                parcel: origin,
                level,
                as_ingredient: false,
            };

            if let ImposterSpecState::Ready(ready) = self.get_spec(&substitute_imposter, benefit) {
                let mut needs_load = false;
                let imposter_data = match ready.imposter_data.as_ref() {
                    Some((spec, path)) => {
                        if let Some(handle) = self.ipfas.asset_server().get_handle(path.as_path()) {
                            Some((handle, *spec))
                        } else {
                            needs_load = true;
                            None
                        }
                    }
                    None => None,
                };

                let floor_data = match ready.floor_data.as_ref() {
                    Some(path) => {
                        if let Some(handle) = self.ipfas.asset_server().get_handle(path.as_path()) {
                            Some(handle)
                        } else {
                            needs_load = true;
                            None
                        }
                    }
                    None => None,
                };

                needs_load |= imposter_data.as_ref().is_some_and(|(handle, _)| {
                    !self
                        .ipfas
                        .asset_server()
                        .is_loaded_with_dependencies(handle.id())
                });

                needs_load |= floor_data.as_ref().is_some_and(|handle| {
                    !self
                        .ipfas
                        .asset_server()
                        .is_loaded_with_dependencies(handle.id())
                });

                if needs_load {
                    debug!("checking fallback {req:?} -> {substitute_imposter:?} failed");
                    let load_request = self
                        .data
                        .requested_loading_handles
                        .entry(substitute_imposter)
                        .or_insert_with(|| AssetLoadRequest::new(ready));
                    load_request.benefit += benefit;
                    #[cfg(debug_assertions)]
                    load_request
                        .debug
                        .push((*req, current_error, distance, parcel_benefit));
                } else {
                    debug!("checking fallback {req:?} -> {substitute_imposter:?} -> ok!",);
                    return ImposterState::PendingWithSubstitute(
                        substitute_imposter,
                        imposter_data,
                        floor_data,
                        parcel_err,
                    );
                }
            }
        }
        let new_error = (6 - req.level) * (6 - req.level) * parcel_count;
        ImposterState::Pending(new_error)
    }

    fn end_tick(&mut self) {
        let ImposterManagerData {
            loading_mips,
            loading_scenes,
            just_removed,
            scenes,
            mips,
            loading_handles,
            requested_loading_handles,
            ..
        } = &mut *self.data;

        // clear unused imposter entities
        for ent in just_removed.drain().flat_map(|(_, ents)| ents.into_iter()) {
            if let Ok(mut commands) = self.commands.get_entity(ent) {
                commands.despawn();
            }
        }

        debug!("{} requested", requested_loading_handles.len());

        // start loading available local assets
        let mut new_loading_handles = HashMap::default();
        // keep prior loads that are still required
        for (imposter, loading_handle) in loading_handles.drain() {
            if requested_loading_handles.contains_key(&imposter) {
                new_loading_handles.insert(imposter, loading_handle);
            }
        }

        // then start new loads up to MAX_ASSET_LOADS by priority
        let count = new_loading_handles.len();

        let mut requests = requested_loading_handles.drain().collect::<Vec<_>>();
        requests.sort_by_key(|(_, req)| FloatOrd(-req.benefit));

        #[cfg(debug_assertions)]
        debug!(
            "candidates: {:?}",
            requests
                .iter()
                .map(|(k, req)| { format!("{k:?}: {} ({:?})", req.benefit, req.debug) })
                .collect::<Vec<_>>()
        );

        new_loading_handles.extend(
            requests
                .into_iter()
                .take(MAX_ASSET_LOADS - new_loading_handles.len())
                .take_while(|(_, req)| req.benefit > 0.0)
                .map(|(imposter, req)| {
                    let imposter_handle = req.spec.imposter_data.map(|(_, path)| {
                        self.ipfas
                            .asset_server()
                            .load_with_settings::<Imposter, ImposterLoaderSettings>(
                                path.as_path(),
                                move |s| {
                                    *s = ImposterLoaderSettings {
                                        multisample: false,
                                        alpha: 1.0,
                                        alpha_blend: 0.0, // blend
                                        multisample_amount: 0.0,
                                        immediate_upload: true,
                                    }
                                },
                            )
                            .untyped()
                    });

                    let floor_handle = req.spec.floor_data.map(|path| {
                        self.ipfas
                            .asset_server()
                            .load::<FloorImposter>(path.as_path())
                            .untyped()
                    });

                    (imposter, (imposter_handle, floor_handle))
                }),
        );
        let count2 = new_loading_handles.len();
        *loading_handles = std::mem::take(&mut new_loading_handles);
        debug!("loading {} / {}", count, count2);

        if self.plugin.download {
            // count current downloads
            let active = loading_mips
                .values()
                .chain(loading_scenes.values())
                .filter(|v| matches!(v, ImposterSpecLoadState::Remote(_)))
                .count();
            let mut free = MAX_ASSET_DOWNLOADS - active;

            #[derive(Debug)]
            enum MipOrScene {
                Mip((IVec2, usize, u32), f32),
                Scene(String, f32),
            }

            impl MipOrScene {
                fn benefit(&self) -> f32 {
                    match self {
                        MipOrScene::Mip(_, d) => *d,
                        MipOrScene::Scene(_, d) => *d,
                    }
                }
            }

            let mut pending_remotes = Vec::new();

            // take all pending remotes, gather and sort those with positive benefit
            let (pending, mut loading): (HashMap<_, _>, HashMap<_, _>) = loading_mips
                .drain()
                .partition(|kv| matches!(kv.1, ImposterSpecLoadState::PendingRemote(_)));
            *loading_mips = std::mem::take(&mut loading);
            pending_remotes.extend(pending.into_iter().filter_map(|(k, v)| {
                let ImposterSpecLoadState::PendingRemote(benefit) = v else {
                    panic!()
                };
                (benefit > 0.0).then_some(MipOrScene::Mip(k, benefit))
            }));

            let (pending, mut loading): (HashMap<_, _>, HashMap<_, _>) = loading_scenes
                .drain()
                .partition(|kv| matches!(kv.1, ImposterSpecLoadState::PendingRemote(_)));
            *loading_scenes = std::mem::take(&mut loading);
            pending_remotes.extend(pending.into_iter().filter_map(|(k, v)| {
                let ImposterSpecLoadState::PendingRemote(benefit) = v else {
                    panic!()
                };
                (benefit > 0.0).then_some(MipOrScene::Scene(k, benefit))
            }));

            pending_remotes.sort_by_key(|mos| -FloatOrd(mos.benefit()));

            // debug!("pending remotes: {:?}", pending_remotes);

            // start new remote fetches
            let mut active_regions: Vec<(IVec2, IVec2)> = Vec::default();
            for mos in pending_remotes.into_iter() {
                if free == 0 {
                    break;
                }

                match mos {
                    MipOrScene::Mip((parcel, level, crc), _benefit) => {
                        let parcel_upper = parcel + (1 << level);
                        if active_regions.iter().any(|(prev, prev_upper)| {
                            (parcel.cmpge(*prev).all() && parcel_upper.cmple(*prev_upper).all())
                                || (prev.cmpge(parcel).all()
                                    && prev_upper.cmple(parcel_upper).all())
                        }) {
                            debug!("skipping {parcel}, {level} due to existing");
                            continue;
                        }

                        loading_mips.insert(
                            (parcel, level, crc),
                            ImposterSpecLoadState::Remote(ImposterLoadTask::new_mip(
                                &self.ipfas,
                                &self.current_realm.about_url,
                                parcel,
                                level,
                                crc,
                                true,
                            )),
                        );

                        active_regions.push((parcel, parcel_upper));
                        debug!("took mip {:?}, benefit {}", (parcel, level, crc), _benefit);
                    }
                    MipOrScene::Scene(hash, _benefit) => {
                        debug!("took scene {:?}, benefit {}", hash, _benefit);
                        let task = ImposterSpecLoadState::Remote(ImposterLoadTask::new_scene(
                            &self.ipfas,
                            &hash,
                            true,
                        ));
                        loading_scenes.insert(hash, task);
                    }
                }

                free -= 1;
            }
        } else {
            // quickly fail everything remote
            loading_scenes.retain(|hash, v| {
                if matches!(v, ImposterSpecLoadState::PendingRemote(_)) {
                    debug!("auto-failing remote for scene {hash}");
                    scenes.insert(hash.clone(), ImposterSpecResolveState::Missing);
                    false
                } else {
                    true
                }
            });
            loading_mips.retain(|key, v| {
                if matches!(v, ImposterSpecLoadState::PendingRemote(_)) {
                    debug!("auto-failing remote for mip {key:?}");
                    mips.insert(*key, ImposterSpecResolveState::Missing);
                    false
                } else {
                    true
                }
            });
        }
    }
}

#[derive(Component)]
pub struct RetryImposter(LevelError);

#[derive(Component)]
pub struct SubstituteImposter(SceneImposter);

fn load_imposters(
    mut commands: Commands,
    pending_imposters: Query<
        (
            Entity,
            &SceneImposter,
            Option<&SubstituteImposter>,
            Option<&RetryImposter>,
        ),
        Or<(Changed<SceneImposter>, With<RetryImposter>)>,
    >,
    mut manager: ImposterSpecManager,
    imposter_meshes: Res<ImposterMeshes>,
    mut meshes: ResMut<Assets<Mesh>>,
    tick: Res<FrameCount>,
    _children: Query<&Children>,
) {
    for (entity, base_imposter, maybe_substitute, maybe_error) in pending_imposters.iter() {
        let state = manager.get_imposter(base_imposter, maybe_error.map(|e| e.0));
        let (scene_imposter, maybe_imposter, maybe_floor, error) = match state {
            ImposterState::Missing => {
                commands
                    .entity(entity)
                    .remove::<RetryImposter>()
                    .despawn_related::<Children>();
                continue;
            }
            ImposterState::Pending(error) => {
                commands.entity(entity).insert(RetryImposter(error));
                continue;
            }
            ImposterState::Ready(imposter, floor) => {
                commands.entity(entity).remove::<RetryImposter>();
                (*base_imposter, imposter, floor, 0)
            }
            ImposterState::PendingWithSubstitute(scene_imposter, imposter, floor, error) => {
                // debug!("wanted {base_imposter:?}, got {scene_imposter:?}");
                (scene_imposter, imposter, floor, error)
            }
            ImposterState::PendingWithPrevious(ents, error) => {
                for prev_child in ents {
                    commands.entity(prev_child).insert(ChildOf(entity));

                    // if let Ok(children) = _children.get(prev_child) {
                    //     for child in children {
                    //         commands.entity(*child).insert(ShowAabbGizmo {
                    //             color: Some(Color::linear_rgba(1.0, 1.0, 0.0, 0.4)),
                    //         });
                    //     }
                    // }
                }
                commands.entity(entity).insert(RetryImposter(error));
                continue;
            }
        };

        if error != 0 {
            if let Some(substitute) = maybe_substitute {
                if substitute.0 == scene_imposter {
                    debug!("skip on repeat sub");
                    continue;
                }
            }
            commands
                .entity(entity)
                .insert((RetryImposter(error), SubstituteImposter(scene_imposter)));
        }

        commands.entity(entity).despawn_related::<Children>();

        let layer = if base_imposter.as_ingredient {
            IMPOSTERCEPTION_LAYER
        } else {
            RenderLayers::default()
        };
        debug!(
            "[{}] spawn imposter: {:?} {:?} {:?}",
            tick.0, base_imposter, error, scene_imposter
        );

        // let color = if error == 0 {
        //     Some(Color::linear_rgba(0.0, 0.0, 1.0, 0.4))
        // } else {
        //     Some(Color::linear_rgba(1.0, 0.0, 0.0, 0.4))
        // };

        commands
            .entity(entity)
            .despawn_related::<Children>()
            .with_children(|c| {
                if let Some((imposter, spec)) = maybe_imposter {
                    let (mesh, aabb) = if error == 0 {
                        (imposter_meshes.cube.clone(), imposter_meshes.aabb)
                    } else {
                        let mesh = ImposterMesh::from_spec(&spec, base_imposter);
                        let aabb = mesh.compute_aabb().unwrap();

                        (meshes.add(mesh), aabb)
                    };

                    let mut scale = spec.region_max - spec.region_min;
                    scale.y = spec.scale * 2.0;
                    c.spawn((
                        Mesh3d(mesh),
                        MeshMaterial3d(imposter),
                        Transform::from_translation((spec.region_min + spec.region_max) * 0.5)
                            .with_scale(
                                scale.max(Vec3::splat(0.001))
                                    * (1.0 + scene_imposter.level as f32 / 1000.0),
                            ),
                        // ShowAabbGizmo { color },
                        aabb,
                        NoFrustumCulling,
                        NotShadowCaster,
                        NotShadowReceiver,
                        layer.clone(),
                    ));
                }

                if let Some(floor) = maybe_floor {
                    //spawn floor
                    let base_size = (1i32 << base_imposter.level) as f32 * PARCEL_SIZE;
                    let mid = (base_imposter.parcel * IVec2::new(1, -1)).as_vec2() * PARCEL_SIZE
                        + Vec2::new(base_size, -base_size) * 0.5;

                    let mesh = if error == 0 {
                        imposter_meshes.floor.clone()
                    } else {
                        let mut floor = Plane3d {
                            normal: Dir3::Y,
                            half_size: Vec2::splat(0.5),
                        }
                        .mesh()
                        .build();

                        let Some(VertexAttributeValues::Float32x2(uvs)) =
                            floor.attribute_mut(Mesh::ATTRIBUTE_UV_0)
                        else {
                            panic!()
                        };

                        let base_size = 1i32 << base_imposter.level;
                        let scene_size = 1i32 << scene_imposter.level;
                        let parcel_size = base_size as f32 / scene_size as f32;
                        let bottomleft = (base_imposter.parcel - scene_imposter.parcel).as_vec2()
                            / scene_size as f32;

                        for uv in uvs.iter_mut() {
                            uv[0] = 0.0 / 18.0 + 17.0 / 18.0 * (bottomleft.x + uv[0] * parcel_size);
                            uv[1] = 0.0 / 18.0
                                + 17.0 / 18.0 * (1.0 - bottomleft.y - (1.0 - uv[1]) * parcel_size);
                        }
                        floor.immediate_upload = true;

                        meshes.add(floor)
                    };

                    c.spawn((
                        Transform::from_translation(Vec3::new(mid.x, -0.01, mid.y))
                            .with_scale(Vec3::new(base_size, 1.0, base_size)),
                        Mesh3d(mesh),
                        Aabb::from_min_max(Vec3::NEG_ONE * 0.49, Vec3::ONE * 0.49),
                        // ShowAabbGizmo { color },
                        MeshMaterial3d(floor),
                        NotShadowCaster,
                        NotShadowReceiver,
                        layer,
                    ));
                }
            });
    }
}

fn debug_write_imposters(
    assets: Res<Assets<Imposter>>,
    tick: Res<FrameCount>,
    mut debug: ResMut<DebugInfo>,
    done: Query<&SceneImposter, Without<RetryImposter>>,
    pending: Query<&SceneImposter, With<RetryImposter>>,
) {
    if tick.0.is_multiple_of(100) {
        return;
    }

    let mut count = 0;
    let mut memory: usize = 0;
    for (_, imposter) in assets.iter() {
        count += 1;
        memory += imposter.vram_bytes;
    }

    debug.info.insert(
        "Imposter memory",
        format!(
            "{} bytes ({} mb) over {} imposters",
            memory,
            memory / 1024 / 1024,
            count
        ),
    );

    let done = done.iter().count();
    let pending = pending.iter().count();
    debug.info.insert(
        "Imposter states",
        format!("done: {done}, pending: {pending}"),
    );
}
