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
use boimp::{
    asset_loader::ImposterVertexMode, bake::ImposterBakeMaterialPlugin, render::Imposter,
    ImposterLoaderSettings,
};
use common::{
    structs::{AppConfig, PrimaryUser},
    util::TaskExt,
};
use ipfs::{ChangeRealmEvent, CurrentRealm, IpfsAssetServer};

use scene_runner::{
    initialize_scene::{LiveScenes, PointerResult, ScenePointers, PARCEL_SIZE},
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
        .init_resource::<ImposterEntitiesTransitioningOut>()
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
pub struct ImposterTransitionOut(bool);

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
    imposters: Query<'w, 's, (Option<&'static ImposterMissing>, Option<&'static Children>)>,
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

impl<'w, 's> ImposterLookup<'w, 's> {
    fn imposter_state(
        entities: &HashMap<(IVec2, usize, bool), Entity>,
        imposters: &Query<(Option<&ImposterMissing>, Option<&Children>)>,
        handles: &Query<(Option<&Handle<Imposter>>, Option<&Handle<FloorImposter>>)>,
        asset_server: &AssetServer,
        parcel: IVec2,
        level: usize,
        ingredient: bool,
    ) -> ImposterState {
        let Some(entity) = entities.get(&(parcel, level, ingredient)) else {
            return ImposterState::NotSpawned;
        };

        let Ok((maybe_missing, maybe_children)) = imposters.get(*entity) else {
            return ImposterState::Pending;
        };

        if let Some(missing) = maybe_missing {
            if missing.0.is_some() || level > 0 {
                return ImposterState::Missing;
            } else {
                return ImposterState::NoScene;
            }
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

#[derive(Resource, Default)]
pub struct ImposterEntitiesTransitioningOut(pub HashMap<(IVec2, usize), Entity>);

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
        )))
    }

    pub fn new_mip(ipfas: &IpfsAssetServer, address: &str, parcel: IVec2, level: usize) -> Self {
        Self(IoTaskPool::get().spawn(load_imposter(
            ipfas.ipfs().clone(),
            address.to_string(),
            parcel,
            level,
        )))
    }
}

#[derive(Resource, Default)]
pub struct BakingIngredients(pub Vec<(IVec2, usize)>);

pub fn spawn_imposters(
    mut commands: Commands,
    mut lookup: ResMut<ImposterEntities>,
    mut transitioning_out: ResMut<ImposterEntitiesTransitioningOut>,
    config: Res<AppConfig>,
    focus: Query<&GlobalTransform, With<PrimaryUser>>,
    mut required: Local<HashSet<(IVec2, usize, bool)>>,
    realm_changed: EventReader<ChangeRealmEvent>,
    ingredients: Res<BakingIngredients>,
    pointers: Res<ScenePointers>,
    imposters: Query<(Option<&ImposterMissing>, Option<&Children>)>,
    handles: Query<(Option<&Handle<Imposter>>, Option<&Handle<FloorImposter>>)>,
    asset_server: Res<AssetServer>,
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
    if pointers.min() == IVec2::MIN {
        return;
    }

    required.extend(ingredients.0.iter().map(|(p, l)| (*p, *l, true)));

    let Some(origin) = focus.get_single().ok().map(|gt| gt.translation().xz()) else {
        return;
    };
    let origin = origin * Vec2::new(1.0, -1.0);

    // gather required
    let mut prev_distance = 0.0;
    for (level, &next_distance) in config.scene_imposter_distances.iter().enumerate() {
        let tile_size = 1 << level;
        let next_tile_size = tile_size / 2;
        let tile_size_world = (tile_size * 16) as f32;

        let min_tile = ((origin - next_distance) / 16.0 / tile_size as f32)
            .floor()
            .as_ivec2();
        let max_tile = ((origin + next_distance) / 16.0 / tile_size as f32)
            .ceil()
            .as_ivec2();

        let min_tile = min_tile.max((pointers.min() & !(tile_size - 1)) / tile_size);
        let max_tile = max_tile.min((pointers.max() & !(tile_size - 1)) / tile_size);

        for x in min_tile.x..=max_tile.x {
            for y in min_tile.y..=max_tile.y {
                let tile_origin_parcel = IVec2::new(x, y) * tile_size;
                let tile_origin_world = tile_origin_parcel.as_vec2() * 16.0;

                let closest_point =
                    origin.clamp(tile_origin_world, tile_origin_world + tile_size_world);
                let closest_distance = (closest_point - origin).length();
                let furthest_distance = (origin - tile_origin_world)
                    .abs()
                    .max((origin - (tile_origin_world + tile_size_world)).abs())
                    .length();

                // println!("tile {tile_origin_parcel}:{level}, tile world: {tile_origin_world}, origin: {origin}, distance [{closest_distance}, {furthest_distance}], prev_distance {prev_distance}, next_distance {next_distance}");

                if closest_distance >= prev_distance && closest_distance < next_distance {
                    // println!("adding");
                    required.insert((tile_origin_parcel, level, false));
                } else if closest_distance < prev_distance && furthest_distance > prev_distance {
                    // println!("adding children");
                    // this tile crosses the boundary, make sure the next level down includes all tiles we are not including here
                    // next level will need these tiles since we don't
                    for offset in [IVec2::ZERO, IVec2::X, IVec2::Y, IVec2::ONE] {
                        let smaller_tile_origin = tile_origin_parcel + offset * next_tile_size;
                        if smaller_tile_origin.clamp(pointers.min(), pointers.max())
                            == smaller_tile_origin
                        {
                            required.insert((smaller_tile_origin, level - 1, false));
                        }
                    }
                };
            }
        }

        prev_distance = next_distance;
    }

    // remove old
    let prev_entities = lookup.0.clone();
    lookup.0.retain(|&(pos, level, ingredient), ent| {
        let mut required = required.remove(&(pos, level, ingredient));

        if !required {
            debug!("remove {}: {} [{}]", level, pos, ingredient);
            if let Some(mut commands) = commands.get_entity(*ent) {
                if ingredient {
                    commands.despawn_recursive();
                    return false;
                }

                commands.try_insert(ImposterTransitionOut(false));
                transitioning_out.0.insert((pos, level), *ent);

                let tile_size = 1 << level;
                let smaller_tile_size = tile_size / 2;
                let larger_tile_size = tile_size * 2;

                let world_min = pos.as_vec2() * PARCEL_SIZE;
                let world_max = world_min + IVec2::splat(tile_size).as_vec2() * PARCEL_SIZE;
                let distance = (origin.clamp(world_min, world_max) - origin).length();

                // check smaller
                if level > 0 && distance < *config.scene_imposter_distances.get(level - 1).unwrap_or(&0.0) {
                    if level > 2 && distance < *config.scene_imposter_distances.get(level - 2).unwrap_or(&0.0) {
                        // skip checks for 2 levels
                    } else {
                        for offset in [IVec2::ZERO, IVec2::X, IVec2::Y, IVec2::ONE] {
                            let smaller = pos + offset * smaller_tile_size;
                            if smaller.clamp(pointers.min(), pointers.max()) == smaller {
                                match ImposterLookup::imposter_state(&prev_entities, &imposters, &handles, &asset_server, smaller, level - 1, false) {
                                    ImposterState::NotSpawned |
                                    ImposterState::Pending => {
                                        debug!("not despawning {}:{} because smaller {}:{} is {:?}", pos, level, smaller, level-1, ImposterLookup::imposter_state(&prev_entities, &imposters, &handles, &asset_server, smaller, level - 1, false));
                                        required = true
                                    }
                                    ImposterState::Ready |
                                    ImposterState::Missing |
                                    ImposterState::NoScene  => (),
                                }
                            }
                        }
                    }
                }

                // check larger
                if distance > *config.scene_imposter_distances.get(level).unwrap_or(&0.0) && level < config.scene_imposter_distances.len() - 1 {
                    if level < config.scene_imposter_distances.len() - 2 && distance > *config.scene_imposter_distances.get(level + 1).unwrap_or(&0.0) {
                        // skip checks for 2 levels
                    } else {
                        let larger = pos & !(larger_tile_size - 1);
                        match ImposterLookup::imposter_state(&prev_entities, &imposters, &handles, &asset_server, larger, level + 1, false) {
                            ImposterState::NotSpawned |
                            ImposterState::Pending => {
                                debug!("(dist {} vs range {:?}) not despawning {}:{} because larger {}:{} is {:?}", distance, config.scene_imposter_distances.get(level), pos, level, larger, level+1, ImposterLookup::imposter_state(&prev_entities, &imposters, &handles, &asset_server, larger, level + 1, false));
                                required = true
                            }
                            ImposterState::Ready |
                            ImposterState::Missing |
                            ImposterState::NoScene => (),
                        }
                    }
                }

                if !required {
                    commands.try_insert(ImposterTransitionOut(true));
                }
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

        if !as_ingredient {
            if let Some(ent) = transitioning_out.0.remove(&(parcel, level)) {
                if let Some(commands) = commands.get_entity(ent) {
                    commands.despawn_recursive();
                }
            }
        }
    }
}

#[derive(Component, Clone)]
pub struct ImposterReady(pub Option<String>);

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
    scene_pointers: Res<ScenePointers>,
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
        } else {
            commands
                .entity(ent)
                .remove::<RetryImposter>()
                .try_insert(ImposterLoadTask::new_mip(
                    &ipfas,
                    &current_realm.address,
                    imposter.parcel,
                    imposter.level,
                ));
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
                            commands.try_insert(ImposterReady(Some(hash.clone())));
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
                        commands.try_insert(ImposterReady(None));
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
                    ready.0.as_ref().unwrap_or(&current_realm.address),
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
                                        vertex_mode: ImposterVertexMode::Billboard,
                                        multisample,
                                        use_source_uv_y: false,
                                        alpha: initial_alpha,
                                        alpha_blend: 0.0, // blend
                                    }
                                },
                            ),
                        transform: Transform::from_translation(
                            (spec.region_min + spec.region_max) * 0.5,
                        )
                        .with_scale(scale * (1.0 + req.level as f32 / 1000.0)),
                        ..Default::default()
                    },
                    NoFrustumCulling,
                    NotShadowCaster,
                    NotShadowReceiver,
                    layer.clone(),
                    ready.clone(),
                ));
            }

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
                ready.0.as_ref().unwrap_or(&current_realm.address),
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
            .0
            .as_ref()
            // either a non-scene mip, or not a live scene, or live and translation != 0 (i.e. tick < 10)
            .map_or(true, |hash| {
                !live_scenes.0.get(hash).map_or(false, |e| {
                    transform.get(*e).map_or(false, |t| t.translation.y == 0.0)
                })
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
    q_in: Query<
        (
            Entity,
            &SceneImposter,
            &Children,
            Has<ImposterTransitionOut>,
        ),
        With<ImposterTransitionIn>,
    >,
    q_out: Query<(Entity, &SceneImposter, &Children, &ImposterTransitionOut)>,
    handles: Query<&Handle<Imposter>>,
    mut lookup: ResMut<ImposterEntitiesTransitioningOut>,
    mut assets: ResMut<Assets<Imposter>>,
    time: Res<Time>,
    mut debug: ResMut<DebugInfo>,
) {
    let mut dont_transition_out = Vec::default();

    for (ent, imp, children, transitioning_out) in q_in.iter() {
        if transitioning_out {
            commands.entity(ent).remove::<ImposterTransitionIn>();
            continue;
        }
        let mut ready = true;

        let size = 1 << imp.level;
        let parent_tile = imp.parcel & !(size * 2 - 1);
        if let Some(parent) = lookup.0.get(&(parent_tile, imp.level + 1)) {
            dont_transition_out.push(*parent);
            if !q_out.get(*parent).map_or(true, |(_, _, _, t)| t.0) {
                // skip while parent isn't ready to despawn
                ready = false;
            }
        }

        if imp.level > 0 {
            for offset in [IVec2::ZERO, IVec2::X, IVec2::Y, IVec2::ONE] {
                let child_tile = imp.parcel + offset * (size >> 1);
                if let Some(child) = lookup.0.get(&(child_tile, imp.level - 1)) {
                    dont_transition_out.push(*child);
                    if !q_out.get(*child).map_or(true, |(_, _, _, t)| t.0) {
                        // skip while child isn't ready to despawn
                        ready = false;
                    }
                }
            }
        }

        if ready {
            let mut still_transitioning = false;
            for child in children {
                if let Ok(h_in) = handles.get(*child) {
                    let Some(asset) = assets.get_mut(h_in.id()) else {
                        still_transitioning = true;
                        continue;
                    };

                    asset.data.alpha =
                        1f32.min(asset.data.alpha + time.delta_seconds() / TRANSITION_TIME);
                    if asset.data.alpha < 1.0 {
                        still_transitioning = true;
                    }
                }
            }

            if !still_transitioning {
                commands.entity(ent).remove::<ImposterTransitionIn>();
            }
        }
    }

    let mut t_out_false = 0;
    let mut t_out_blocked = 0;
    let mut t_out_true = 0;
    for (ent, imposter, children, t) in q_out.iter() {
        if t.0 && dont_transition_out.contains(&ent) {
            t_out_blocked += 1;
        } else if t.0 {
            t_out_true += 1;
        } else {
            t_out_false += 1;
        }

        if t.0 && !dont_transition_out.contains(&ent) {
            let mut still_transitioning = false;
            for child in children {
                if let Ok(h_out) = handles.get(*child) {
                    let Some(asset) = assets.get_mut(h_out.id()) else {
                        continue;
                    };

                    asset.data.alpha =
                        0f32.max(asset.data.alpha - time.delta_seconds() / TRANSITION_TIME);
                    if asset.data.alpha > 0.0 {
                        still_transitioning = true;
                    }
                }
            }

            if !still_transitioning {
                commands.entity(ent).despawn_recursive();
                lookup.0.remove(&(imposter.parcel, imposter.level));
            }
        }
    }

    let in_count = q_in.iter().count();
    if in_count > 0 {
        debug.info.insert("Trans In", format!("{in_count}"));
    } else {
        debug.info.remove("Trans In");
    }
    let out_count = q_out.iter().count();
    if out_count > 0 {
        debug.info.insert(
            "Trans Out",
            format!("t: {t_out_true}, b: {t_out_blocked}, f: {t_out_false}"),
        );
    } else {
        debug.info.remove("Trans Out");
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
