use bevy::{
    core::FrameCount,
    pbr::{NotShadowCaster, NotShadowReceiver},
    prelude::*,
    render::mesh::VertexAttributeValues,
    tasks::{IoTaskPool, Task},
    utils::{hashbrown::HashSet, HashMap},
};
use boimp::{asset_loader::ImposterVertexMode, render::Imposter, ImposterLoaderSettings};
use common::{structs::PrimaryUser, util::TaskExt};
use ipfs::IpfsAssetServer;

use scene_runner::initialize_scene::{LiveScenes, PointerResult, ScenePointers};

use crate::{
    floor_imposter::{FloorImposter, FloorImposterLoader},
    imposter_spec::{
        load_scene_imposter, scene_floor_path, scene_texture_path, BakedScene, ImposterSpec,
    },
};

pub struct DclImposterRenderPlugin;

impl Plugin for DclImposterRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((MaterialPlugin::<FloorImposter>::default(),))
            .init_resource::<ImposterLoadDistance>()
            .init_resource::<ImposterLookup>()
            .init_asset_loader::<FloorImposterLoader>()
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (
                    spawn_imposters,
                    load_imposters,
                    render_imposters,
                    update_imposter_visibility,
                    debug_write_imposters,
                ),
            );
    }
}

#[derive(Resource)]
struct ImposterMeshes {
    cube: Handle<Mesh>,
    floor: Handle<Mesh>,
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    let mut cube = Cuboid::default().mesh().build();
    let Some(VertexAttributeValues::Float32x2(uvs)) = cube.attribute_mut(Mesh::ATTRIBUTE_UV_0)
    else {
        panic!()
    };

    for ix in [0, 1, 6, 7, 8, 11, 12, 15, 20, 21, 22, 23] {
        uvs[ix][1] = 0.0;
    }
    for ix in [2, 3, 4, 5, 9, 10, 13, 14, 16, 17, 18, 19] {
        uvs[ix][1] = 1.0;
    }
    let cube = meshes.add(cube);
    commands.insert_resource(ImposterMeshes {
        cube,
        floor: meshes.add(Plane3d {
            normal: Dir3::Y,
            half_size: Vec2::splat(0.5),
        }),
    })
}

#[derive(Component)]
pub struct ImposterTransitionIn;

#[derive(Component)]
pub struct ImposterTransitionOut;

pub const TRANSITION_TIME: f32 = 1.25;

#[derive(Resource, Default)]
pub struct ImposterLookup(pub HashMap<(IVec2, usize), Entity>);

#[derive(Resource, Default)]
pub struct ImposterLoadDistance(pub Vec<f32>);

#[derive(Component, Debug)]
pub struct SceneImposter {
    pub parcel: IVec2,
    pub level: usize,
}

#[derive(Component)]
pub struct SceneImposterLoadTask(Task<Option<BakedScene>>);

impl SceneImposterLoadTask {
    pub fn new_scene(ipfas: &IpfsAssetServer, scene_hash: &str) -> Self {
        Self(IoTaskPool::get().spawn(load_scene_imposter(
            ipfas.ipfs().clone(),
            scene_hash.to_string(),
        )))
    }
}

fn spawn_imposters(
    mut commands: Commands,
    mut lookup: ResMut<ImposterLookup>,
    load_distance: Res<ImposterLoadDistance>,
    focus: Query<&GlobalTransform, With<PrimaryUser>>,
    mut required: Local<HashSet<(IVec2, usize)>>,
) {
    let Some(origin) = focus.get_single().ok().map(|gt| gt.translation().xz()) else {
        return;
    };
    let origin = origin * Vec2::new(1.0, -1.0);

    // gather required
    let mut prev_distance = 0.0;
    for (level, &next_distance) in load_distance.0.iter().enumerate() {
        let tile_size = 1 << level;
        let next_tile_size = tile_size / 2;
        let tile_size_world = (tile_size * 16) as f32;

        let min_parcel = ((origin - next_distance) / 16.0 / tile_size as f32).as_ivec2();
        let max_parcel = ((origin + next_distance) / 16.0 / tile_size as f32)
            .ceil()
            .as_ivec2();

        for x in min_parcel.x..=max_parcel.x {
            for y in min_parcel.y..=max_parcel.y {
                let tile_origin = IVec2::new(x, y) * tile_size;
                let tile_origin_world = tile_origin.as_vec2() * 16.0;

                let closest_point =
                    origin.clamp(tile_origin_world, tile_origin_world + tile_size_world);
                let closest_distance = (closest_point - origin).length();
                let furthest_distance = (origin - tile_origin_world)
                    .abs()
                    .max((origin - (tile_origin_world + tile_size_world)).abs())
                    .length();

                if closest_distance >= prev_distance && closest_distance < next_distance {
                    required.insert((tile_origin, level));
                } else if closest_distance < prev_distance && furthest_distance > prev_distance {
                    // this tile crosses the boundary, make sure the next level down includes all tiles we are not including here
                    // next level will need these tiles since we don't
                    for offset in [IVec2::ZERO, IVec2::X, IVec2::Y, IVec2::ONE] {
                        let smaller_tile_origin = tile_origin + offset * next_tile_size;
                        required.insert((smaller_tile_origin, level - 1));
                    }
                };
            }
        }

        prev_distance = next_distance;
    }

    // remove old
    lookup.0.retain(|&(pos, level), ent| {
        let required = required.remove(&(pos, level));
        if !required {
            if let Some(mut commands) = commands.get_entity(*ent) {
                commands.try_insert(ImposterTransitionOut);
                // for now
                commands.despawn_recursive();
            }
        }
        required
    });

    // add new
    for (parcel, level) in required.drain() {
        let id = commands
            .spawn((SpatialBundle::default(), SceneImposter { parcel, level }))
            .id();

        println!("require {}", parcel);
        lookup.0.insert((parcel, level), id);
    }
}

#[derive(Component)]
pub struct ImposterReady(pub Option<String>);

#[derive(Component)]
pub struct ImposterMissing(pub Option<String>);

#[derive(Component)]
pub struct RetryImposter;

fn load_imposters(
    mut commands: Commands,
    mut loading_scenes: Local<HashMap<String, (SceneImposterLoadTask, Vec<Entity>)>>,
    new_imposters: Query<
        (Entity, &SceneImposter),
        Or<(Changed<SceneImposter>, With<RetryImposter>)>,
    >,
    all_imposters: Query<&SceneImposter>,
    scene_pointers: Res<ScenePointers>,
    ipfas: IpfsAssetServer,
) {
    // create any new load tasks
    for (ent, imposter) in new_imposters.iter() {
        if imposter.level == 0 {
            match scene_pointers.0.get(&imposter.parcel) {
                Some(PointerResult::Exists { hash, .. }) => {
                    loading_scenes
                        .entry(hash.clone())
                        .or_insert_with(|| {
                            (
                                SceneImposterLoadTask::new_scene(&ipfas, hash),
                                Vec::default(),
                            )
                        })
                        .1
                        .push(ent);
                    commands.entity(ent).remove::<RetryImposter>();
                }
                Some(_) => {
                    commands.entity(ent).remove::<RetryImposter>();
                }
                None => {
                    // retry next frame
                    commands.entity(ent).try_insert(RetryImposter);
                }
            }
        }
    }

    // process tasks
    (*loading_scenes).retain(|hash, (task, entities)| {
        match task.0.complete() {
            Some(res) => {
                if let Some(mut scene) = res {
                    println!("load success {hash}");
                    // loaded successfully
                    for entity in entities.iter() {
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
                    println!("load fail {hash}");
                    for entity in entities.iter() {
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
) {
    // spawn/update required
    for (entity, req, maybe_spec, ready) in new_imposters.iter() {
        println!("spawn imposter {:?} {:?}", req, maybe_spec);
        if req.level == 0 {
            commands.entity(entity).with_children(|c| {
                if let Some(spec) = maybe_spec {
                    // spawn imposter
                    let path =
                        scene_texture_path(ipfas.ipfs(), ready.0.as_ref().unwrap(), req.parcel);
                    let mut scale = spec.region_max - spec.region_min;
                    scale.y = spec.scale * 2.0;
                    c.spawn((
                        MaterialMeshBundle {
                            mesh: imposter_meshes.cube.clone(),
                            material: asset_server
                                .load_with_settings::<Imposter, ImposterLoaderSettings>(
                                    path,
                                    |s| {
                                        *s = ImposterLoaderSettings {
                                            vertex_mode: ImposterVertexMode::NoBillboard,
                                            multisample: true,
                                            use_source_uv_y: true,
                                            alpha: 1.0,
                                        }
                                    },
                                ),
                            transform: Transform::from_translation(
                                (spec.region_min + spec.region_max) * 0.5,
                            )
                            .with_scale(scale),
                            ..Default::default()
                        },
                        ImposterTransitionIn,
                        NotShadowCaster,
                        NotShadowReceiver,
                    ));
                }

                //spawn floor
                let size = ((1i32 << req.level) * 16) as f32;
                let mid =
                    (req.parcel * IVec2::new(1, -1) * 16).as_vec2() + Vec2::new(size, -size) * 0.5;

                let path = scene_floor_path(ipfas.ipfs(), ready.0.as_ref().unwrap(), req.parcel);
                c.spawn((
                    MaterialMeshBundle {
                        transform: Transform::from_translation(Vec3::new(mid.x, -0.01, mid.y))
                            .with_scale(Vec3::new(size + 2.0, 1.0, size + 2.0)),
                        mesh: imposter_meshes.floor.clone(),
                        material: asset_server.load::<FloorImposter>(path),
                        ..Default::default()
                    },
                    NotShadowCaster,
                    NotShadowReceiver,
                ));
            });
        }
    }
}

fn update_imposter_visibility(
    mut q: Query<(&mut Visibility, &ImposterReady)>,
    live_scenes: Res<LiveScenes>,
) {
    for (mut vis, ready) in q.iter_mut() {
        let show = ready
            .0
            .as_ref()
            .map_or(true, |hash| !live_scenes.0.contains_key(hash));
        *vis = if show {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

fn debug_write_imposters(assets: Res<Assets<Imposter>>, tick: Res<FrameCount>) {
    if tick.0 % 100 != 0 {
        return;
    }

    let mut count = 0;
    let /*mut*/ memory: usize = 0;
    let /*mut*/ memory_compressed: usize = 0;
    for (_, _imposter) in assets.iter() {
        count += 1;
        // memory += imposter.base_size as usize;
        // memory_compressed += imposter.compressed_size as usize;
    }

    info!(
        "{} bytes ({} mb) over {} imposters",
        memory,
        memory / 1024 / 1024,
        count
    );
    info!(
        "{} bytes ({} mb) over {} imposters tho actually",
        memory_compressed,
        memory_compressed / 1024 / 1024,
        count
    );
}
