use std::path::PathBuf;

use bevy::{
    diagnostic::FrameCount,
    ecs::system::SystemParam,
    pbr::{NotShadowCaster, NotShadowReceiver},
    platform::collections::{HashMap, HashSet},
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
        .init_resource::<ImposterSpecs>()
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
                // render_imposters,
                // update_imposter_visibility,
                // transition_imposters,
                debug_write_imposters,
            )
                .chain()
                .in_set(SceneSets::PostLoop),
        )
        .add_systems(
            PostUpdate,
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

#[derive(Resource, Default)]
pub struct ImposterFocus(Vec2);

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
    let focus_player = (cam_pos - player_pos).length() < 20.0;
    let focus_player = focus_player || {
        let player_angle = Transform::from_translation(cam_pos)
            .looking_at(player_pos, Vec3::Y)
            .rotation;
        let angle_between = cam_rot.angle_between(player_angle);
        // debug!("angle between: {angle_between}");
        angle_between < 0.2
    };

    let origin = match focus_player {
        true => player_pos,
        false => {
            let cam_fwd = cam_rot * Vec3::NEG_Z;
            cam_pos - cam_fwd * cam_pos.y / cam_fwd.y
            // debug!("player pos: {player_pos}, ground_intersect: {ground_intersect}");
        }
    };

    focus.0 = origin.xz();
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
    if current_realm.is_changed() {
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

    let origin = focus.0 * Vec2::new(1.0, -1.0);

    // record live parcels
    let current_imposter_scene = match &current_imposter_scene.0 {
        Some((PointerResult::Exists { hash, .. }, _)) => Some(hash),
        _ => None,
    };
    let live_parcels = live_scenes
        .scenes
        .iter()
        .filter(|(hash, _)| Some(*hash) != current_imposter_scene)
        .flat_map(|(_, e)| scenes.get(*e).ok().map(|ctx| &ctx.parcels))
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
            let closest_distance = (closest_point - origin).length();

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
            let is_child = k.parcel.cmpge(parcel).all() && k.parcel.cmplt(max_parcel).all();
            if is_child {
                manager.store_removed(scene_imposter, *v);
            }
            !is_child
        });

        commands.spawn((Transform::default(), Visibility::default(), scene_imposter));

        debug!("require {}: {} [{}]", level, parcel, as_ingredient);
    }

    // remove any not required non-children
    for (_, ent) in old.drain() {
        commands.entity(ent).despawn();
    }
}

#[derive(Component)]
pub struct WrongImposterLevel;

#[derive(Debug)]
pub enum ImposterSpecResolveState {
    Missing,
    PendingRemote,
    Ready(BakedScene),
}

pub enum ImposterSpecLoadState {
    Local(ImposterLoadTask),
    PendingRemote,
    Remote(ImposterLoadTask),
}

#[derive(Resource, Default)]
pub struct ImposterSpecs {
    pub mips: HashMap<(IVec2, usize, u32), ImposterSpecResolveState>,
    pub scenes: HashMap<String, ImposterSpecResolveState>,

    pub loading_mips: HashMap<(IVec2, usize, u32), ImposterSpecLoadState>,
    pub loading_scenes: HashMap<String, ImposterSpecLoadState>,

    pub prev_loading_mips: HashMap<(IVec2, usize, u32), ImposterSpecLoadState>,
    pub prev_loading_scenes: HashMap<String, ImposterSpecLoadState>,

    pub imposter_paths: HashMap<(IVec2, usize, u32), (Option<PathBuf>, Option<PathBuf>)>,
    pub loading_handles: HashSet<UntypedHandle>,
    pub permanent_handles: HashSet<UntypedHandle>,

    pub just_removed: HashMap<SceneImposter, Vec<Entity>>,
}

#[derive(PartialEq)]
pub enum ImposterSpecState {
    Ready(Option<ImposterSpec>, String, bool),
    Pending,
    Missing,
}

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
    ),
    PendingWithPrevious(Vec<Entity>),
    Pending,
    Missing,
}

#[derive(SystemParam)]
pub struct ImposterSpecManager<'w, 's> {
    commands: Commands<'w, 's>,
    specs: ResMut<'w, ImposterSpecs>,
    current_realm: Res<'w, CurrentRealm>,
    pub(crate) pointers: ResMut<'w, ScenePointers>,
    ipfas: IpfsAssetServer<'w, 's>,
    focus: Res<'w, ImposterFocus>,
    plugin: Res<'w, DclImposterPlugin>,
}

impl<'w, 's> ImposterSpecManager<'w, 's> {
    fn clear(&mut self) {
        self.specs.mips.clear();
        self.specs.scenes.clear();
    }

    pub(crate) fn clear_scene(&mut self, hash: &str) {
        self.specs.scenes.remove(hash);
    }

    pub(crate) fn clear_mip(&mut self, parcel: IVec2, level: usize, crc: u32) {
        self.specs.mips.remove(&(parcel, level, crc));
    }

    fn start_tick(&mut self) {
        let dropped_loads = self
            .specs
            .prev_loading_mips
            .values()
            .filter(|l| matches!(l, ImposterSpecLoadState::Remote(_)))
            .count();
        if dropped_loads > 0 {
            debug!("dropped {dropped_loads} loading mips");
        }
        self.specs.prev_loading_mips = std::mem::take(&mut self.specs.loading_mips);
        self.specs.prev_loading_scenes = std::mem::take(&mut self.specs.loading_scenes);
        self.specs.loading_handles.clear();
    }

    fn store_removed(&mut self, child_of: SceneImposter, entity: Entity) {
        self.specs
            .just_removed
            .entry(child_of)
            .or_default()
            .push(entity);
    }

    pub fn get_spec(&mut self, req: &SceneImposter) -> ImposterSpecState {
        let ImposterSpecs {
            mips,
            scenes,
            loading_mips,
            loading_scenes,
            prev_loading_mips,
            prev_loading_scenes,
            ..
        } = &mut *self.specs;

        let resolve = |spec: Option<&ImposterSpecResolveState>,
                       path: &str,
                       has_floor: bool|
         -> ImposterSpecState {
            match spec {
                Some(ImposterSpecResolveState::Missing) => ImposterSpecState::Missing,
                Some(ImposterSpecResolveState::PendingRemote) | None => ImposterSpecState::Pending,
                Some(ImposterSpecResolveState::Ready(baked_scene)) => ImposterSpecState::Ready(
                    baked_scene.imposters.get(&req.parcel).copied(),
                    path.to_owned(),
                    has_floor,
                ),
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

            if let Some((hash, mut load_state)) = prev_loading_scenes.remove_entry(hash) {
                match load_state {
                    ImposterSpecLoadState::Local(ref mut imposter_load_task) => {
                        if let Some(maybe_baked_scene) = imposter_load_task.0.complete() {
                            let new_state = match maybe_baked_scene {
                                Some(baked_scene) => ImposterSpecResolveState::Ready(baked_scene),
                                None => ImposterSpecResolveState::PendingRemote,
                            };
                            debug!("end local fetch {hash}: {new_state:?}");
                            scenes.insert(hash, new_state);
                        } else {
                            loading_scenes.insert(hash, load_state);
                        }
                    }
                    ImposterSpecLoadState::Remote(ref mut imposter_load_task) => {
                        if let Some(maybe_baked_scene) = imposter_load_task.0.complete() {
                            let new_state = match maybe_baked_scene {
                                Some(baked_scene) => ImposterSpecResolveState::Ready(baked_scene),
                                None => ImposterSpecResolveState::Missing,
                            };
                            scenes.insert(hash, new_state);
                        } else {
                            loading_scenes.insert(hash, load_state);
                        }
                    }
                    ImposterSpecLoadState::PendingRemote => (),
                }
            } else if !loading_scenes.contains_key(hash) {
                match resolve_state {
                    Some(ImposterSpecResolveState::PendingRemote) => {
                        debug!("requesting remote fetch {hash}");
                        loading_scenes.insert(hash.clone(), ImposterSpecLoadState::PendingRemote);
                    }
                    None => {
                        debug!("start local fetch {hash}");
                        loading_scenes.insert(
                            hash.clone(),
                            ImposterSpecLoadState::Local(ImposterLoadTask::new_scene(
                                &self.ipfas,
                                hash,
                                false,
                            )),
                        );
                    }
                    _ => panic!(),
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
                    ImposterSpecLoadState::PendingRemote => (),
                }
            } else if !loading_mips.contains_key(&key) {
                match resolve_state {
                    Some(ImposterSpecResolveState::PendingRemote) => {
                        debug!("requesting remote fetch {req:?}");
                        loading_mips.insert(key, ImposterSpecLoadState::PendingRemote);
                    }
                    None => {
                        debug!("start local fetch {req:?}");
                        loading_mips.insert(
                            key,
                            ImposterSpecLoadState::Local(ImposterLoadTask::new_mip(
                                &self.ipfas,
                                &self.current_realm.about_url,
                                req.parcel,
                                req.level,
                                crc,
                                false,
                            )),
                        );
                    }
                    _ => panic!(),
                }
            }

            resolve(mips.get(&key), &self.current_realm.about_url, crc != 0)
        }
    }

    pub fn get_imposter(&mut self, req: &SceneImposter) -> ImposterState {
        let spec_state = self.get_spec(req);

        let maybe_spec = match spec_state {
            ImposterSpecState::Ready(imposter_spec, id, has_floor) => {
                Some((imposter_spec, id, has_floor))
            }
            ImposterSpecState::Pending => None,
            ImposterSpecState::Missing => return ImposterState::Missing,
        };

        if let Some((maybe_spec, id, has_floor)) = maybe_spec {
            let crc = self.pointers.crc(req.parcel, req.level).unwrap();

            // get paths
            let ImposterSpecs {
                imposter_paths,
                loading_handles,
                ..
            } = &mut *self.specs;

            let paths = imposter_paths
                .entry((req.parcel, req.level, crc))
                .or_insert_with(|| {
                    (
                        maybe_spec.map(|_| {
                            texture_path(self.ipfas.ipfs_cache_path(), &id, req.parcel, req.level)
                        }),
                        has_floor.then(|| {
                            floor_path(self.ipfas.ipfs_cache_path(), &id, req.parcel, req.level)
                        }),
                    )
                });

            // start loading if required
            let imposter_handle = paths.0.as_deref().map(|p| {
                self.ipfas.asset_server().get_handle(p).unwrap_or_else(|| {
                    let handle = self
                        .ipfas
                        .asset_server()
                        .load_with_settings::<Imposter, ImposterLoaderSettings>(p, move |s| {
                            *s = ImposterLoaderSettings {
                                multisample: false,
                                alpha: 1.0,
                                alpha_blend: 0.0, // blend
                                multisample_amount: 0.0,
                                immediate_upload: true,
                            }
                        });
                    loading_handles.insert(handle.clone().untyped());
                    handle
                })
            });

            let floor_handle = paths.1.as_deref().map(|p| {
                self.ipfas.asset_server().get_handle(p).unwrap_or_else(|| {
                    let handle = self.ipfas.asset_server().load::<FloorImposter>(p);
                    loading_handles.insert(handle.clone().untyped());
                    handle
                })
            });

            if imposter_handle.as_ref().is_none_or(|handle| {
                self.ipfas
                    .asset_server()
                    .is_loaded_with_dependencies(handle.id())
            }) && floor_handle.as_ref().is_none_or(|handle| {
                self.ipfas
                    .asset_server()
                    .is_loaded_with_dependencies(handle.id())
            }) {
                return ImposterState::Ready(
                    maybe_spec.map(|s| (imposter_handle.unwrap(), s)),
                    floor_handle,
                );
            }
        }

        // not downloaded or not spawned, fallback to best available
        if req.as_ingredient {
            return ImposterState::Pending;
        }

        // check for previously despawned lower mip entities first
        if let Some(entities) = self.specs.just_removed.remove(req) {
            return ImposterState::PendingWithPrevious(entities);
        }

        // check for larger mips
        for level in req.level + 1..=5 {
            // use mask to knock out the lower bits, this works for negative numbers too
            let origin = req.parcel & !((1 << level) - 1);
            let Some(crc) = self.pointers.crc(origin, level) else {
                return ImposterState::Pending;
            };

            let substitute_imposter = SceneImposter {
                parcel: origin,
                level,
                as_ingredient: false,
            };
            if let Some((imposter_path, floor_path)) =
                self.specs.imposter_paths.get(&(origin, level, crc))
            {
                debug!("checking fallback {req:?} -> {substitute_imposter:?}");

                let mut ready = true;

                let imposter_handle = match imposter_path.as_deref() {
                    Some(p) => {
                        if let Some(handle) = self.ipfas.asset_server().get_handle(p) {
                            ready &= self
                                .ipfas
                                .asset_server()
                                .is_loaded_with_dependencies(handle.id());
                            Some(handle)
                        } else {
                            ready = false;
                            None
                        }
                    }
                    None => None,
                };

                let floor_handle = match floor_path.as_deref() {
                    Some(p) => {
                        if let Some(handle) = self.ipfas.asset_server().get_handle(p) {
                            ready &= self
                                .ipfas
                                .asset_server()
                                .is_loaded_with_dependencies(handle.id());
                            Some(handle)
                        } else {
                            ready = false;
                            None
                        }
                    }
                    None => None,
                };

                if ready {
                    let imposter_and_spec = imposter_handle.map(|h| {
                        let ImposterSpecState::Ready(maybe_spec, _, _) =
                            self.get_spec(&substitute_imposter)
                        else {
                            panic!();
                        };
                        (h, maybe_spec.unwrap())
                    });

                    debug!("checking fallback {req:?} -> {substitute_imposter:?} -> ok!");
                    return ImposterState::PendingWithSubstitute(
                        substitute_imposter,
                        imposter_and_spec,
                        floor_handle,
                    );
                } else {
                    debug!("checking fallback {req:?} -> {substitute_imposter:?} failed")
                }
            }
        }

        // we only reach here if actually requested with level 5
        ImposterState::Pending
    }

    fn end_tick(&mut self) {
        let ImposterSpecs {
            loading_mips,
            loading_scenes,
            just_removed,
            scenes,
            mips,
            ..
        } = &mut *self.specs;

        for ent in just_removed.drain().flat_map(|(_, ents)| ents.into_iter()) {
            if let Ok(mut commands) = self.commands.get_entity(ent) {
                commands.despawn();
            }
        }

        if self.plugin.download {
            let active = loading_mips
                .values()
                .chain(loading_scenes.values())
                .filter(|v| matches!(v, ImposterSpecLoadState::Remote(_)))
                .count();
            let mut free = 20 - active;

            if free == 0 {
                return;
            }

            for (hash, v) in loading_scenes
                .iter_mut()
                .filter(|(_, v)| matches!(v, ImposterSpecLoadState::PendingRemote))
                .take(free)
            {
                debug!("started scene remote: {hash}");
                *v = ImposterSpecLoadState::Remote(ImposterLoadTask::new_scene(
                    &self.ipfas,
                    hash,
                    true,
                ));
                free -= 1;
            }

            if free == 0 {
                return;
            }

            let focus = self.focus.0.as_ivec2() * IVec2::new(1, -1) / PARCEL_SIZE as i32;

            let mut mips = loading_mips
                .iter_mut()
                .filter(|(_, v)| matches!(v, ImposterSpecLoadState::PendingRemote))
                .map(|(k, v)| {
                    let &(parcel, level, _) = k;
                    let midpoint = parcel + IVec2::splat(1 << (level - 1));
                    let distance = (midpoint - focus).length_squared();
                    ((distance, (usize::MAX - level)), (k, v))
                })
                .collect::<Vec<_>>();
            mips.sort_by_key(|(sort_key, _)| *sort_key);

            for (_, (k, v)) in mips.into_iter().take(free) {
                debug!("started mip remote {k:?}");
                *v = ImposterSpecLoadState::Remote(ImposterLoadTask::new_mip(
                    &self.ipfas,
                    &self.current_realm.about_url,
                    k.0,
                    k.1,
                    k.2,
                    true,
                ));
                free -= 1;
            }
        } else {
            // quickly fail everything remote
            loading_scenes.retain(|hash, v| {
                if matches!(v, ImposterSpecLoadState::PendingRemote) {
                    debug!("auto-failing remote for scene {hash}");
                    scenes.insert(hash.clone(), ImposterSpecResolveState::Missing);
                    false
                } else {
                    true
                }
            });
            loading_mips.retain(|key, v| {
                if matches!(v, ImposterSpecLoadState::PendingRemote) {
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
pub struct RetryImposter;

#[derive(Component)]
pub struct SubstituteImposter(SceneImposter);

fn load_imposters(
    mut commands: Commands,
    pending_imposters: Query<
        (Entity, &SceneImposter, Option<&SubstituteImposter>),
        Or<(Changed<SceneImposter>, With<RetryImposter>)>,
    >,
    mut manager: ImposterSpecManager,
    imposter_meshes: Res<ImposterMeshes>,
    mut meshes: ResMut<Assets<Mesh>>,
    tick: Res<FrameCount>,
    _children: Query<&Children>,
) {
    for (entity, base_imposter, maybe_substitute) in pending_imposters.iter() {
        let state = manager.get_imposter(base_imposter);
        let (scene_imposter, maybe_imposter, maybe_floor, is_final) = match state {
            ImposterState::Missing => {
                commands
                    .entity(entity)
                    .remove::<RetryImposter>()
                    .despawn_related::<Children>();
                continue;
            }
            ImposterState::Pending => {
                commands.entity(entity).insert(RetryImposter);
                continue;
            }
            ImposterState::Ready(imposter, floor) => {
                commands.entity(entity).remove::<RetryImposter>();
                (*base_imposter, imposter, floor, true)
            }
            ImposterState::PendingWithSubstitute(scene_imposter, imposter, floor) => {
                // debug!("wanted {base_imposter:?}, got {scene_imposter:?}");
                (scene_imposter, imposter, floor, false)
            }
            ImposterState::PendingWithPrevious(ents) => {
                for prev_child in ents {
                    commands.entity(prev_child).insert(ChildOf(entity));

                    // if let Ok(children) = _children.get(prev_child) {
                    //     for child in children {
                    //         commands.entity(*child).insert(ShowAabbGizmo {
                    //             color: Some(Color::linear_rgba(1.0, 1.0, 0.0, 0.7)),
                    //         });
                    //     }
                    // }
                }
                commands.entity(entity).insert(RetryImposter);
                continue;
            }
        };

        if !is_final {
            if let Some(substitute) = maybe_substitute {
                if substitute.0 == scene_imposter {
                    debug!("skip on repeat sub");
                    continue;
                }
            }
            commands
                .entity(entity)
                .insert((RetryImposter, SubstituteImposter(scene_imposter)));
        }

        commands.entity(entity).despawn_related::<Children>();

        let layer = if base_imposter.as_ingredient {
            IMPOSTERCEPTION_LAYER
        } else {
            RenderLayers::default()
        };
        debug!(
            "[{}] spawn imposter: {:?} {:?} {:?}",
            tick.0, base_imposter, is_final, scene_imposter
        );

        // let color = if is_final {!
        //     Some(Color::linear_rgba(0.0, 0.0, 1.0, 0.7))
        // } else {
        //     Some(Color::linear_rgba(1.0, 0.0, 0.0, 0.7))
        // };

        commands
            .entity(entity)
            .despawn_related::<Children>()
            .with_children(|c| {
                if let Some((imposter, spec)) = maybe_imposter {
                    let (mesh, aabb) = if is_final {
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

                    let mesh = if is_final {
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
