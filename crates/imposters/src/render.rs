use bevy::{
    asset::LoadState,
    core::FrameCount,
    ecs::system::SystemParam,
    pbr::{NotShadowCaster, NotShadowReceiver},
    prelude::*,
    render::{
        mesh::VertexAttributeValues,
        view::{NoFrustumCulling, RenderLayers},
    },
    tasks::{IoTaskPool, Task},
    utils::{hashbrown::HashSet, HashMap},
};
use boimp::{bake::ImposterBakeMaterialPlugin, render::Imposter, ImposterLoaderSettings};
use common::{
    structs::{AppConfig, PrimaryUser},
    util::TaskExt,
};
use crc::CRC_32_CKSUM;
use ipfs::{ChangeRealmEvent, CurrentRealm, IpfsAssetServer};

use scene_runner::{
    initialize_scene::{LiveScenes, PointerResult, SceneLoading, ScenePointers, PARCEL_SIZE},
    renderer_context::RendererSceneContext,
    DebugInfo,
};

use crate::{
    bake_scene::IMPOSTERCEPTION_LAYER,
    floor_imposter::{FloorImposter, FloorImposterLoader},
    imposter_spec::{floor_path, load_imposter, texture_path, BakedScene, ImposterSpec},
};

pub struct DclImposterRenderPlugin;

impl Plugin for DclImposterRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            MaterialPlugin::<FloorImposter>::default(),
            ImposterBakeMaterialPlugin::<FloorImposter>::default(),
        ))
        .init_resource::<ImposterEntities>()
        .init_resource::<BakingIngredients>()
        .init_asset_loader::<FloorImposterLoader>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                spawn_imposters,
                load_imposters,
                render_imposters,
                update_imposter_visibility,
                transition_imposters,
                debug_write_imposters,
            )
                .chain(),
        );
    }
}

#[derive(Resource)]
struct ImposterMeshes {
    cube: Handle<Mesh>,
    floor: Handle<Mesh>,
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    // let mut cube = Cuboid::default().mesh().build();
    // let Some(VertexAttributeValues::Float32x2(uvs)) = cube.attribute_mut(Mesh::ATTRIBUTE_UV_0)
    // else {
    //     panic!()
    // };

    // for ix in [0, 1, 6, 7, 8, 11, 12, 15, 20, 21, 22, 23] {
    //     uvs[ix][1] = 0.0;
    // }
    // for ix in [2, 3, 4, 5, 9, 10, 13, 14, 16, 17, 18, 19] {
    //     uvs[ix][1] = 1.0;
    // }
    // let cube = meshes.add(cube);

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

    commands.insert_resource(ImposterMeshes {
        cube: meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5))),
        // cube,
        floor: meshes.add(floor),
    })
}

#[derive(Component)]
pub struct ImposterTransitionIn;

#[derive(Component)]
pub struct ImposterTransitionOut;

pub const TRANSITION_TIME: f32 = 0.25;

#[derive(PartialEq, Debug)]
pub enum ImposterState {
    NotSpawned,
    Pending,
    Ready,
    Missing,
    NoScene,
}

#[derive(SystemParam)]
pub struct ImposterLookup<'w, 's> {
    entities: Res<'w, ImposterEntities>,
    imposters: Query<
        'w,
        's,
        (
            Option<&'static ImposterMissing>,
            Option<&'static Children>,
            Option<&'static ImposterReady>,
        ),
    >,
    handles: Query<
        'w,
        's,
        (
            Option<&'static Handle<Imposter>>,
            Option<&'static Handle<FloorImposter>>,
        ),
    >,
    asset_server: Res<'w, AssetServer>,
}

impl ImposterLookup<'_, '_> {
    fn imposter_state(
        entities: &HashMap<(IVec2, usize, bool), Entity>,
        imposters: &Query<(
            Option<&ImposterMissing>,
            Option<&Children>,
            Option<&ImposterReady>,
        )>,
        handles: &Query<(Option<&Handle<Imposter>>, Option<&Handle<FloorImposter>>)>,
        asset_server: &AssetServer,
        parcel: IVec2,
        level: usize,
        ingredient: bool,
    ) -> ImposterState {
        let Some(entity) = entities.get(&(parcel, level, ingredient)) else {
            return ImposterState::NotSpawned;
        };

        let Ok((maybe_missing, maybe_children, maybe_ready)) = imposters.get(*entity) else {
            return ImposterState::Pending;
        };

        if let Some(missing) = maybe_missing {
            if missing.0.is_some() || level > 0 {
                return ImposterState::Missing;
            } else {
                return ImposterState::NoScene;
            }
        }

        if maybe_ready.is_some_and(|r| r.crc == 0) {
            return ImposterState::Ready;
        }

        let Some(children) = maybe_children else {
            return ImposterState::Pending;
        };

        for child in children {
            let (maybe_wall, maybe_floor) = handles.get(*child).unwrap();
            for id in [
                maybe_wall.map(|h| h.id().untyped()),
                maybe_floor.map(|h| h.id().untyped()),
            ]
            .into_iter()
            .flatten()
            {
                match asset_server.get_load_state(id) {
                    None => return ImposterState::Pending,
                    Some(LoadState::Loading) => return ImposterState::Pending,
                    Some(LoadState::Failed(_)) => return ImposterState::Missing,
                    Some(LoadState::NotLoaded) => return ImposterState::Missing,
                    Some(LoadState::Loaded) => (),
                }
            }
        }

        ImposterState::Ready
    }

    pub fn state(&self, parcel: IVec2, size: usize, ingredient: bool) -> ImposterState {
        Self::imposter_state(
            &self.entities.0,
            &self.imposters,
            &self.handles,
            &self.asset_server,
            parcel,
            size,
            ingredient,
        )
    }
}

#[derive(Resource, Default)]
pub struct ImposterEntities(pub HashMap<(IVec2, usize, bool), Entity>);

#[derive(Component, Debug)]
pub struct SceneImposter {
    pub parcel: IVec2,
    pub level: usize,
    pub as_ingredient: bool,
}

#[derive(Component)]
pub struct ImposterLoadTask(Task<Option<BakedScene>>);

impl ImposterLoadTask {
    pub fn new_scene(ipfas: &IpfsAssetServer, scene_hash: &str) -> Self {
        Self(IoTaskPool::get().spawn(load_imposter(
            ipfas.ipfs().clone(),
            scene_hash.to_string(),
            IVec2::MAX,
            0,
            None, // don't need to check since we load by id
        )))
    }

    pub fn new_mip(
        ipfas: &IpfsAssetServer,
        address: &str,
        parcel: IVec2,
        level: usize,
        crc: u32,
    ) -> Self {
        Self(IoTaskPool::get().spawn(load_imposter(
            ipfas.ipfs().clone(),
            address.to_string(),
            parcel,
            level,
            Some(crc),
        )))
    }
}

#[derive(Resource, Default)]
pub struct BakingIngredients(pub Vec<(IVec2, usize)>);

pub fn spawn_imposters(
    mut commands: Commands,
    mut lookup: ResMut<ImposterEntities>,
    config: Res<AppConfig>,
    focus: Query<&GlobalTransform, With<PrimaryUser>>,
    mut required: Local<HashSet<(IVec2, usize, bool)>>,
    realm_changed: EventReader<ChangeRealmEvent>,
    ingredients: Res<BakingIngredients>,
    pointers: Res<ScenePointers>,
    live_scenes: Res<LiveScenes>,
    scenes: Query<&RendererSceneContext, Without<SceneLoading>>,
) {
    if !realm_changed.is_empty() {
        for (_, entity) in lookup.0.drain() {
            if let Some(commands) = commands.get_entity(entity) {
                commands.despawn_recursive();
            }
        }

        return;
    }

    // skip if no realm
    if pointers.min() == IVec2::MAX {
        return;
    }

    // add baking requirements
    required.extend(ingredients.0.iter().map(|(p, l)| (*p, *l, true)));

    let Some(origin) = focus.get_single().ok().map(|gt| gt.translation().xz()) else {
        return;
    };
    let origin = origin * Vec2::new(1.0, -1.0);

    // record live parcels
    let live_parcels = live_scenes
        .0
        .values()
        .flat_map(|e| scenes.get(*e).ok().map(|ctx| &ctx.parcels))
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

    let min_tile = min_tile.max(pointers.min() >> level as u32);
    let max_tile = max_tile.min(pointers.max() >> level as u32);

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
            render_tile &= closest_distance > config.scene_imposter_distances[level - 1];
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
                debug!("adding {}:{} == {}", tile, level, tile_origin_parcel);
                required.insert((tile_origin_parcel, level, false));
            } else {
                // add to next level requirements
                debug!("cant' add {}:{} == {}", tile, level, tile_origin_parcel);
                for offset in [IVec2::ZERO, IVec2::X, IVec2::Y, IVec2::ONE] {
                    debug!("maybe the child {}:{}", tile * 2 + offset, level - 1);
                    required_tiles.insert(tile * 2 + offset);
                }
            }
        }

        level -= 1;
    }

    for remaining_parcel in required_tiles {
        if !live_parcels.contains(&remaining_parcel) {
            required.insert((remaining_parcel, 0, false));
        }
    }

    // remove old
    lookup.0.retain(|&(pos, level, ingredient), ent| {
        let required = required.remove(&(pos, level, ingredient));

        if !required {
            debug!("remove {}: {} [{}]", level, pos, ingredient);
            if let Some(mut commands) = commands.get_entity(*ent) {
                if ingredient {
                    commands.despawn_recursive();
                    return false;
                }

                commands.try_insert(ImposterTransitionOut);
            }
        }
        required
    });

    // add new
    for (parcel, level, as_ingredient) in required.drain() {
        let mut cmds = commands.spawn((
            SpatialBundle::default(),
            SceneImposter {
                parcel,
                level,
                as_ingredient,
            },
        ));
        if !as_ingredient {
            cmds.insert(ImposterTransitionIn);
        }

        debug!("require {}: {} [{}]", level, parcel, as_ingredient);
        lookup.0.insert((parcel, level, as_ingredient), cmds.id());
    }
}

#[derive(Component, Clone)]
pub struct ImposterReady {
    pub scene: Option<String>,
    pub crc: u32,
}

#[derive(Component)]
pub struct ImposterMissing(pub Option<String>);

#[derive(Component)]
pub struct RetryImposter;

fn load_imposters(
    mut commands: Commands,
    mut loading_scenes: Local<HashMap<String, (ImposterLoadTask, Vec<(Entity, IVec2)>)>>,
    mut loading_parcels: Query<(Entity, &mut ImposterLoadTask, &SceneImposter)>,
    new_imposters: Query<
        (Entity, &SceneImposter),
        Or<(Changed<SceneImposter>, With<RetryImposter>)>,
    >,
    all_imposters: Query<&SceneImposter>,
    mut scene_pointers: ResMut<ScenePointers>,
    ipfas: IpfsAssetServer,
    current_realm: Res<CurrentRealm>,
) {
    // create any new load tasks
    for (ent, imposter) in new_imposters.iter() {
        if imposter.level == 0 {
            match scene_pointers.get(imposter.parcel) {
                Some(PointerResult::Exists { hash, .. }) => {
                    loading_scenes
                        .entry(hash.clone())
                        .or_insert_with(|| {
                            (ImposterLoadTask::new_scene(&ipfas, hash), Vec::default())
                        })
                        .1
                        .push((ent, imposter.parcel));
                    commands.entity(ent).remove::<RetryImposter>();
                }
                Some(_) => {
                    commands
                        .entity(ent)
                        .remove::<RetryImposter>()
                        .insert(ImposterMissing(None));
                }
                None => {
                    // retry next frame
                    commands.entity(ent).try_insert(RetryImposter);
                }
            }
        } else if current_realm.address.is_empty() {
            commands.entity(ent).try_insert(RetryImposter);
        } else if let Some(crc) = scene_pointers.crc(imposter.parcel, imposter.level) {
            commands
                .entity(ent)
                .remove::<RetryImposter>()
                .try_insert(ImposterLoadTask::new_mip(
                    &ipfas,
                    &current_realm.address,
                    imposter.parcel,
                    imposter.level,
                    crc,
                ));
        } else {
            commands.entity(ent).try_insert(RetryImposter);
        }
    }

    // process tasks
    (*loading_scenes).retain(|hash, (task, entities)| {
        match task.0.complete() {
            Some(res) => {
                if let Some(mut scene) = res {
                    debug!("load success {hash}");
                    // loaded successfully
                    for (entity, parcel) in entities.iter() {
                        debug!(" @ {parcel}");
                        if let Ok(imposter) = all_imposters.get(*entity) {
                            let mut commands = commands.entity(*entity);

                            if let Some(spec) = scene.imposters.remove(&imposter.parcel) {
                                commands.try_insert(spec);
                            }
                            commands.try_insert(ImposterReady {
                                scene: Some(hash.clone()),
                                crc: crc::Crc::<u32>::new(&CRC_32_CKSUM).checksum(hash.as_bytes()),
                            });
                        }
                    }
                } else {
                    // didn't exist
                    debug!("load fail {hash}");
                    for (entity, parcel) in entities.iter() {
                        debug!(" @ {parcel}");
                        if let Some(mut commands) = commands.get_entity(*entity) {
                            commands.try_insert(ImposterMissing(Some(hash.clone())));
                        }
                    }
                }
                false
            }
            None => true,
        }
    });

    for (ent, mut task, imposter) in loading_parcels.iter_mut() {
        if let Some(res) = task.0.complete() {
            if let Some(mut commands) = commands.get_entity(ent) {
                match res {
                    Some(mut baked) => {
                        debug!("load success {:?}", imposter);
                        if let Some(spec) = baked.imposters.remove(&imposter.parcel) {
                            commands.try_insert(spec);
                        }
                        commands.try_insert(ImposterReady {
                            scene: None,
                            crc: baked.crc,
                        });
                    }
                    None => {
                        // didn't exist
                        debug!("load fail {:?}", imposter);
                        commands.try_insert(ImposterMissing(None));
                    }
                }
                commands.remove::<ImposterLoadTask>();
            };
        }
    }
}

fn render_imposters(
    mut commands: Commands,
    new_imposters: Query<
        (
            Entity,
            &SceneImposter,
            Option<&ImposterSpec>,
            &ImposterReady,
        ),
        Added<ImposterReady>,
    >,
    imposter_meshes: Res<ImposterMeshes>,
    asset_server: Res<AssetServer>,
    ipfas: IpfsAssetServer,
    current_realm: Res<CurrentRealm>,
    config: Res<AppConfig>,
) {
    // spawn/update required
    for (entity, req, maybe_spec, ready) in new_imposters.iter() {
        let (layer, initial_alpha) = if req.as_ingredient {
            (IMPOSTERCEPTION_LAYER, 1.0)
        } else {
            (RenderLayers::default(), 0.0)
        };
        debug!("spawn imposter {:?} {:?}", req, maybe_spec);
        commands.entity(entity).with_children(|c| {
            if let Some(spec) = maybe_spec {
                // spawn imposter
                let path = texture_path(
                    ipfas.ipfs(),
                    ready.scene.as_ref().unwrap_or(&current_realm.address),
                    req.parcel,
                    req.level,
                );
                let mut scale = spec.region_max - spec.region_min;
                scale.y = spec.scale * 2.0;
                let multisample = config.scene_imposter_multisample;
                c.spawn((
                    MaterialMeshBundle {
                        mesh: imposter_meshes.cube.clone(),
                        material: asset_server
                            .load_with_settings::<Imposter, ImposterLoaderSettings>(
                                path,
                                move |s| {
                                    *s = ImposterLoaderSettings {
                                        multisample,
                                        alpha: initial_alpha,
                                        alpha_blend: 0.0, // blend
                                    }
                                },
                            ),
                        transform: Transform::from_translation(
                            (spec.region_min + spec.region_max) * 0.5,
                        )
                        .with_scale(
                            scale.max(Vec3::splat(0.001)) * (1.0 + req.level as f32 / 1000.0),
                        ),
                        ..Default::default()
                    },
                    NoFrustumCulling,
                    NotShadowCaster,
                    NotShadowReceiver,
                    layer.clone(),
                    ready.clone(),
                ));
            }

            if ready.crc != 0 {
                //spawn floor
                let offset = match req.level {
                    0 => 0.0,
                    _ => 0.0,
                };
                let size = (1i32 << req.level) as f32 * PARCEL_SIZE;
                let mid = (req.parcel * IVec2::new(1, -1)).as_vec2() * PARCEL_SIZE
                    + Vec2::new(size, -size) * 0.5;

                let path = floor_path(
                    ipfas.ipfs(),
                    ready.scene.as_ref().unwrap_or(&current_realm.address),
                    req.parcel,
                    req.level,
                );
                c.spawn((
                    MaterialMeshBundle {
                        transform: Transform::from_translation(Vec3::new(mid.x, -0.01, mid.y))
                            .with_scale(Vec3::new(size, 1.0, size)),
                        mesh: imposter_meshes.floor.clone(),
                        material: asset_server
                            .load_with_settings::<FloorImposter, f32>(path, move |s: &mut f32| {
                                *s = offset
                            }),
                        ..Default::default()
                    },
                    NotShadowCaster,
                    NotShadowReceiver,
                    layer,
                    ready.clone(),
                ));
            }
        });
    }
}

fn update_imposter_visibility(
    mut q: Query<(&mut RenderLayers, &ImposterReady)>,
    live_scenes: Res<LiveScenes>,
    transform: Query<&Transform>,
) {
    for (mut layers, ready) in q.iter_mut() {
        let show = ready
            .scene
            .as_ref()
            // either a non-scene mip, or not a live scene, or live and translation != 0 (i.e. tick < 10)
            .map_or(true, |hash| {
                !live_scenes
                    .0
                    .get(hash)
                    .is_some_and(|e| transform.get(*e).is_ok_and(|t| t.translation.y == 0.0))
            });
        *layers = if show {
            layers.clone().with(0)
        } else {
            layers.clone().without(0)
        };
    }
}

fn transition_imposters(
    mut commands: Commands,
    q_in: Query<(Entity, &Children, Has<ImposterTransitionOut>), With<ImposterTransitionIn>>,
    q_out: Query<(Entity, &Children), With<ImposterTransitionOut>>,
    handles: Query<&Handle<Imposter>>,
    mut assets: ResMut<Assets<Imposter>>,
    time: Res<Time>,
) {
    const TPOW: f32 = 2.0;

    for (ent, children, transitioning_out) in q_in.iter() {
        if transitioning_out {
            commands.entity(ent).remove::<ImposterTransitionIn>();
            continue;
        }

        let mut still_transitioning = false;
        for child in children {
            if let Ok(h_in) = handles.get(*child) {
                let Some(asset) = assets.get_mut(h_in.id()) else {
                    still_transitioning = true;
                    continue;
                };

                asset.data.alpha = 1f32
                    .min(asset.data.alpha.powf(TPOW) + time.delta_seconds() / TRANSITION_TIME)
                    .powf(TPOW.recip());
                if asset.data.alpha < 1.0 {
                    still_transitioning = true;
                }
            }
        }

        if !still_transitioning {
            commands.entity(ent).remove::<ImposterTransitionIn>();
        }
    }

    for (ent, children) in q_out.iter() {
        let mut still_transitioning = false;
        for child in children {
            if let Ok(h_out) = handles.get(*child) {
                let Some(asset) = assets.get_mut(h_out.id()) else {
                    continue;
                };

                asset.data.alpha = 0f32
                    .max(asset.data.alpha.powf(TPOW) - time.delta_seconds() / TRANSITION_TIME)
                    .powf(TPOW.recip());
                if asset.data.alpha > 0.0 {
                    still_transitioning = true;
                }
            }
        }

        if !still_transitioning {
            commands.entity(ent).despawn_recursive();
        }
    }
}

fn debug_write_imposters(
    assets: Res<Assets<Imposter>>,
    tick: Res<FrameCount>,
    mut debug: ResMut<DebugInfo>,
) {
    if tick.0 % 100 != 0 {
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
}
