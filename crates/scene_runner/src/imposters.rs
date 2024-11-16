use std::path::PathBuf;

use bevy::{
    asset::AssetLoader,
    core::FrameCount,
    math::FloatOrd,
    pbr::{ExtendedMaterial, MaterialExtension, NotShadowCaster, NotShadowReceiver},
    prelude::*,
    render::{mesh::VertexAttributeValues, primitives::Aabb, render_resource::AsBindGroup},
    utils::HashMap,
};
use boimp::{
    asset_loader::{ImposterLoader, ImposterVertexMode},
    bake::{BakeState, ImposterBakeBundle, ImposterBakeCamera, ImposterMaterialPlugin},
    render::Imposter,
    GridMode, ImposterBakePlugin, ImposterLoaderSettings,
};
use common::structs::{PrimaryUser, SceneLoadDistance};
use ipfs::IpfsAssetServer;
use scene_material::{BoundRegion, SceneBound, SceneMaterial};
use serde::{Deserialize, Serialize};

use crate::{
    initialize_scene::{
        parcels_in_range, CurrentImposterScene, LiveScenes, PointerResult, ScenePointers,
    },
    renderer_context::RendererSceneContext,
};
pub struct ImposterPlugin;

const GRID_SIZE: u32 = 8;
const TILE_SIZE: u32 = 64;
const MAX_IMPOSTER_SIZE: i32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct ImposterSpec {
    scale: f32,
    region_min: Vec3,
    region_max: Vec3,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct ImposterParcels {
    parcel_min: IVec2,
    parcel_max: IVec2,
}

#[derive(Debug, Serialize, Deserialize)]
struct BakedImposter {
    fully_baked: bool,
    imposters: Vec<(ImposterParcels, Option<Option<ImposterSpec>>)>,
}

#[derive(Resource, Default)]
pub struct BakedImposters {
    initialized: bool,
    scene_imposters: HashMap<String, BakedImposter>,
}

impl Plugin for ImposterPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ImposterBakePlugin,
            ImposterMaterialPlugin::<SceneMaterial>::default(),
            MaterialPlugin::<FloorImposter>::default(),
        ))
        .init_asset_loader::<FloorImposterLoader>()
        .init_resource::<BakedImposters>()
        .add_systems(Startup, (setup, load_imposters))
        .add_systems(
            Update,
            (
                pick_imposter_to_bake,
                make_oven,
                bake_imposters,
                render_imposters,
                // debug_write_imposters,
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
pub struct ImposterOven {
    start_tick: u32,
    hash: String,
    unbaked_regions: Vec<BoundRegion>,
}

fn load_imposters(mut baked: ResMut<BakedImposters>, ipfas: IpfsAssetServer) {
    if baked.initialized {
        return;
    }

    let mut path = ipfas.ipfs_cache_path().to_owned();
    path.push("imposters");
    path.push("specs");
    for file in std::fs::read_dir(path)
        .map(Iterator::collect::<Vec<_>>)
        .unwrap_or_default()
        .into_iter()
        .flatten()
    {
        let Some(imposter) = std::fs::File::open(file.path())
            .ok()
            .and_then(|f| serde_json::from_reader(f).ok())
        else {
            warn!("failed to read imposter spec from `{:?}`", file.path());
            continue;
        };

        let filename = file.file_name().to_string_lossy().into_owned();
        let Some((hash, _)) = filename.split_once(".") else {
            warn!("bad filename `{}`", file.file_name().to_string_lossy());
            continue;
        };

        baked.scene_imposters.insert(hash.to_owned(), imposter);
    }

    baked.initialized = true;
}

fn pick_imposter_to_bake(
    mut current_imposter: ResMut<CurrentImposterScene>,
    baked: Res<BakedImposters>,
    scene_pointers: Res<ScenePointers>,
    live_scenes: Res<LiveScenes>,
    load_distance: Res<SceneLoadDistance>,
    focus: Query<&GlobalTransform, With<PrimaryUser>>,
) {
    if !baked.initialized || current_imposter.0.is_some() {
        return;
    }

    let Ok(focus) = focus.get_single() else {
        return;
    };
    let nearest_unbaked = parcels_in_range(focus, load_distance.load_imposter)
        .into_iter()
        .filter_map(|(parcel, distance)| {
            let pointer = scene_pointers.0.get(&parcel);
            pointer
                .and_then(PointerResult::hash_and_urn)
                .map(|(hash, _)| (distance, pointer.unwrap(), hash))
        })
        .filter(|(_, _, hash)| {
            !live_scenes.0.contains_key(hash)
                && !baked
                    .scene_imposters
                    .get(hash)
                    .map_or(false, |b| b.fully_baked)
        })
        .min_by_key(|(distance, ..)| FloatOrd(*distance));

    if let Some((_, pointer, _)) = nearest_unbaked {
        warn!("picked scene {:?}", pointer);
        current_imposter.0 = Some(pointer.clone());
    };
}

fn make_oven(
    current_imposter: Res<CurrentImposterScene>,
    mut commands: Commands,
    baking: Query<&ImposterOven>,
    mut baked: ResMut<BakedImposters>,
    mut scenes: Query<(&mut RendererSceneContext, &mut Transform)>,
    live_scenes: Res<LiveScenes>,
    tick: Res<FrameCount>,
    ipfas: IpfsAssetServer,
) {
    if !baking.is_empty() {
        return;
    }

    let Some(PointerResult::Exists { hash, .. }) = &current_imposter.0 else {
        return;
    };

    if let Some(entity) = live_scenes.0.get(hash) {
        let Ok((mut context, mut transform)) = scenes.get_mut(*entity) else {
            return;
        };

        transform.translation.y = -2000.0;

        if context.tick_number < 10 && !context.broken {
            return;
        }

        context.blocked.insert("imposter_baking");

        // generate regions
        let mut unbaked_regions = Vec::default();
        for bound in &context.bounds {
            let size = bound.parcel_max() - bound.parcel_min() + 1;
            let x_regions = (size.x + MAX_IMPOSTER_SIZE - 1) / MAX_IMPOSTER_SIZE;
            let y_regions = (size.y + MAX_IMPOSTER_SIZE - 1) / MAX_IMPOSTER_SIZE;
            let num_regions = IVec2::new(x_regions, y_regions);

            println!(
                "{:?} : {},{} -> {}",
                bound,
                bound.parcel_min(),
                bound.parcel_max(),
                size
            );

            for x in 0..x_regions {
                for y in 0..y_regions {
                    unbaked_regions.push(BoundRegion::new(
                        bound.parcel_min() + (size * IVec2::new(x, y)) / num_regions,
                        bound.parcel_min() + (size * (IVec2::new(x, y) + 1) / num_regions) - 1,
                        bound.parcel_count,
                    ));
                    warn!(
                        "subregion {}{}",
                        bound.parcel_min() + (size * IVec2::new(x, y)) / num_regions,
                        bound.parcel_min() + (size * (IVec2::new(x, y) + 1) / num_regions) - 1
                    );
                }
            }
        }

        let imposter_spec = BakedImposter {
            fully_baked: false,
            imposters: unbaked_regions
                .clone()
                .into_iter()
                .map(|r| {
                    (
                        ImposterParcels {
                            parcel_min: r.parcel_min(),
                            parcel_max: r.parcel_max(),
                        },
                        None,
                    )
                })
                .collect(),
        };

        // write to disk
        let path = spec_path(&ipfas, hash);
        let _ = std::fs::create_dir_all(path.parent().unwrap());
        if let Err(e) = std::fs::File::create(path)
            .map_err(|e| e.to_string())
            .and_then(|f| serde_json::to_writer(f, &imposter_spec).map_err(|e| e.to_string()))
        {
            warn!("failed to write imposter spec: {e}");
        }

        // add bake def
        baked.scene_imposters.insert(hash.clone(), imposter_spec);

        warn!("baking scene {:?}", hash);
        warn!("regions: {:?}", unbaked_regions);

        // spawn oven
        commands.spawn(ImposterOven {
            start_tick: tick.0,
            hash: hash.clone(),
            unbaked_regions,
        });
    }
}

fn bake_imposters(
    mut commands: Commands,
    mut current_imposter: ResMut<CurrentImposterScene>,
    mut baking: Query<(Entity, &mut ImposterOven)>,
    mut all_baking_cams: Query<(Entity, &mut ImposterBakeCamera)>,
    mut baked: ResMut<BakedImposters>,
    mut scenes: Query<(&mut RendererSceneContext, &mut Transform)>,
    live_scenes: Res<LiveScenes>,
    ipfas: IpfsAssetServer,
    tick: Res<FrameCount>,
    children: Query<&Children>,
    meshes: Query<(&GlobalTransform, &Aabb), With<Handle<Mesh>>>,
    bound_materials: Query<&Handle<SceneMaterial>>,
    mut materials: ResMut<Assets<SceneMaterial>>,
    mut current_spec: Local<Option<(ImposterParcels, ImposterSpec)>>,
) {
    if let Ok((baking_ent, mut oven)) = baking.get_single_mut() {
        let current_scene_ent = {
            let Some(PointerResult::Exists { hash, .. }) = &current_imposter.0 else {
                return;
            };
            let Some(entity) = live_scenes.0.get(hash) else {
                return;
            };
            *entity
        };

        let any_baking_cams = !all_baking_cams.is_empty();
        let all_cams_finished = all_baking_cams
            .iter()
            .all(|(_, cam)| cam.state == BakeState::Finished);

        if !any_baking_cams {
            let Some(region) = oven.unbaked_regions.pop() else {
                warn!("no regions left");
                let imposter = baked.scene_imposters.get_mut(&oven.hash).unwrap();

                imposter.fully_baked = true;
                write_imposter(&ipfas, &oven.hash, imposter);

                if let Ok((mut context, _)) = scenes.get_mut(current_scene_ent) {
                    context.blocked.remove("imposter_baking");
                }
                current_imposter.0 = None;
                commands.entity(baking_ent).despawn_recursive();
                return;
            };
            warn!("baking region: {:?}", region);

            // update materials
            for h_mat in children
                .iter_descendants(current_scene_ent)
                .filter_map(|c| bound_materials.get(c).ok())
            {
                let Some(mat) = materials.get_mut(h_mat) else {
                    continue;
                };
                mat.extension.data = SceneBound::new(vec![region], 0.0).data;
            }

            // region bounds
            let rmin = region.world_min();
            let rmax = region.world_max();

            // content bounds
            let mut points = Vec::default();
            for (gt, aabb) in children
                .iter_descendants(current_scene_ent)
                .filter_map(|e| meshes.get(e).ok())
            {
                let corners = [
                    Vec3::new(-1.0, -1.0, -1.0),
                    Vec3::new(-1.0, -1.0, 1.0),
                    Vec3::new(-1.0, 1.0, -1.0),
                    Vec3::new(-1.0, 1.0, 1.0),
                    Vec3::new(1.0, -1.0, -1.0),
                    Vec3::new(1.0, -1.0, 1.0),
                    Vec3::new(1.0, 1.0, -1.0),
                    Vec3::new(1.0, 1.0, 1.0),
                ];
                let new_points = corners
                    .iter()
                    .map(|c| {
                        gt.transform_point(
                            Vec3::from(aabb.center) + (Vec3::from(aabb.half_extents) * *c),
                        ) + Vec3::Y
                    })
                    .collect::<Vec<_>>();

                // skip low things (max y < 1)
                // skip flat things that won't get rendered
                // skip things that don't intersect our rendered region
                let min = new_points.iter().fold(Vec3::MAX, |m, p| m.min(*p));
                let max = new_points.iter().fold(Vec3::MIN, |m, p| m.max(*p));
                let intersects = max.cmpge(rmin).all() || min.cmple(rmax).all();
                if max.y > -1999.0 && max.y > min.y && intersects {
                    points.extend(new_points);
                }
            }

            // skip empty
            if points.is_empty() {
                warn!("skipping empty bake");
                let imposters = baked.scene_imposters.get_mut(&oven.hash).unwrap();
                let target_imposter = imposters
                    .imposters
                    .iter_mut()
                    .find(|(r, _)| {
                        *r == ImposterParcels {
                            parcel_min: region.parcel_min(),
                            parcel_max: region.parcel_max(),
                        }
                    })
                    .unwrap();
                target_imposter.1 = Some(None);
                write_imposter(&ipfas, &oven.hash, imposters);
            } else {
                let mut content_aabb = Aabb::enclosing(points).unwrap();
                content_aabb.center.y += 2000.0;
                let aabb = Aabb::from_min_max(
                    rmin.max(content_aabb.min().into()),
                    rmax.min(content_aabb.max().into()),
                );
                let center = Vec3::from(aabb.center);
                let radius = aabb.half_extents.length();

                warn!("region: {rmin}-{rmax}, snap: {}-{}", aabb.min(), aabb.max());
                let tile_size = (TILE_SIZE as f32
                    * aabb.half_extents.xz().length().max(aabb.half_extents.y)
                    / 16.0)
                    .clamp(2.0, 256.0) as u32;
                warn!("tile size: {tile_size}");

                let mut camera = ImposterBakeCamera {
                    radius,
                    grid_size: GRID_SIZE,
                    tile_size,
                    grid_mode: GridMode::Horizontal,
                    max_tiles_per_frame: 5,
                    ..Default::default()
                };

                let mut path = ipfas.ipfs_cache_path().to_owned();
                path.push("imposters");
                path.push("textures");
                path.push(format!(
                    "{},{},{},{},{}.boimp",
                    oven.hash,
                    region.parcel_min().x,
                    region.parcel_min().y,
                    region.parcel_max().x,
                    region.parcel_max().y
                ));
                let _ = std::fs::create_dir_all(path.parent().unwrap());
                let callback = camera.save_asset_callback(path, true);
                camera.set_callback(callback);

                commands.spawn(ImposterBakeBundle {
                    camera,
                    transform: Transform::from_translation(center + Vec3::Y * -2000.0),
                    ..Default::default()
                });

                *current_spec = Some((
                    ImposterParcels {
                        parcel_min: region.parcel_min(),
                        parcel_max: region.parcel_max(),
                    },
                    ImposterSpec {
                        scale: radius,
                        region_min: aabb.min().into(),
                        region_max: aabb.max().into(),
                    },
                ));
            }

            // always generate top-down
            let tile_mid = (rmin + rmax) * 0.5;
            let mut top_down = ImposterBakeCamera {
                grid_size: 1,
                radius: (rmax - rmin).xz().max_element() * 0.5 + 1.0,
                tile_size: (rmax - rmin).xz().max_element() as u32 + 2,
                order: -98,
                manual_camera_transforms: Some(vec![Transform::from_translation(Vec3::new(
                    tile_mid.x,
                    -2000.0 + 2.0,
                    tile_mid.z,
                ))
                .looking_at(Vec3::new(tile_mid.x, -2000.0, tile_mid.z), Vec3::NEG_Z)
                .into()]),
                ..Default::default()
            };

            let mut path = ipfas.ipfs_cache_path().to_owned();
            path.push("imposters");
            path.push("textures");
            path.push(format!(
                "{},{},{},{},{}-topdown.boimp",
                oven.hash,
                region.parcel_min().x,
                region.parcel_min().y,
                region.parcel_max().x,
                region.parcel_max().y
            ));
            let callback = top_down.save_asset_callback(path, true);
            top_down.set_callback(callback);

            commands.spawn(ImposterBakeBundle {
                transform: Transform::from_translation(Vec3::new(tile_mid.x, -2000.0, tile_mid.z)),
                camera: top_down,
                ..Default::default()
            });
        } else {
            if tick.0 % 200 == 0 {
                warn!("waiting for bake ...");
            }

            // force failed
            if oven.start_tick + 100 < tick.0 {
                warn!("forcing failed bake ...");
                for (_, mut cam) in all_baking_cams.iter_mut() {
                    cam.wait_for_render = false;
                }
            }

            // save result
            if all_cams_finished {
                warn!("finished baking");

                for (cam_ent, _) in all_baking_cams.iter() {
                    commands.entity(cam_ent).despawn_recursive();
                }

                if let Some((current_parcels, current_spec)) = current_spec.take() {
                    let imposters = baked.scene_imposters.get_mut(&oven.hash).unwrap();
                    let target_imposter = imposters
                        .imposters
                        .iter_mut()
                        .find(|(r, _)| *r == current_parcels)
                        .unwrap();
                    target_imposter.1 = Some(Some(current_spec));
                    write_imposter(&ipfas, &oven.hash, imposters);
                }
            }
        }
    }
}

#[derive(Component)]
pub struct ImposterScene {
    hash: String,
    children: Vec<Entity>,
}

#[derive(Component)]
pub struct ImposterTransition;

pub const TRANSITION_TIME: f32 = 1.25;

#[derive(Clone, AsBindGroup, Asset, TypePath)]
pub struct FloorMaterialExt {}

impl MaterialExtension for FloorMaterialExt {
    fn vertex_shader() -> bevy::render::render_resource::ShaderRef {
        "shaders/floor_vertex.wgsl".into()
    }

    fn fragment_shader() -> bevy::render::render_resource::ShaderRef {
        "shaders/floor_fragment.wgsl".into()
    }
}

pub type FloorImposter = ExtendedMaterial<Imposter, FloorMaterialExt>;

#[derive(Default)]
pub struct FloorImposterLoader;

impl AssetLoader for FloorImposterLoader {
    type Asset = FloorImposter;
    type Settings = ();
    type Error = anyhow::Error;

    fn load<'a>(
        &'a self,
        reader: &'a mut bevy::asset::io::Reader,
        _: &'a Self::Settings,
        load_context: &'a mut bevy::asset::LoadContext,
    ) -> impl bevy::utils::ConditionalSendFuture<Output = Result<Self::Asset, Self::Error>> {
        Box::pin(async move {
            let base = ImposterLoader
                .load(
                    reader,
                    &ImposterLoaderSettings {
                        multisample: true,
                        ..Default::default()
                    },
                    load_context,
                )
                .await?;
            Ok(FloorImposter {
                base,
                extension: FloorMaterialExt {},
            })
        })
    }
}

fn render_imposters(
    mut commands: Commands,
    baked_imposters: Res<BakedImposters>,
    existing_imposters: Query<(Entity, &ImposterScene)>,
    existing_regions: Query<
        (&Handle<Imposter>, Option<&ImposterTransition>),
        With<Handle<Imposter>>,
    >,
    scene_pointers: Res<ScenePointers>,
    focus: Query<&GlobalTransform, With<PrimaryUser>>,
    load_distance: Res<SceneLoadDistance>,
    live_scenes: Res<LiveScenes>,
    scene_transforms: Query<&Transform, With<RendererSceneContext>>,
    imposter_meshes: Res<ImposterMeshes>,
    asset_server: Res<AssetServer>,
    ipfas: IpfsAssetServer,
    mut imposter_assets: ResMut<Assets<Imposter>>,
    time: Res<Time>,
) {
    if !baked_imposters.initialized {
        return;
    }

    let mut required_imposters = HashMap::default();
    let Ok(focus) = focus.get_single() else {
        return;
    };
    let parcels = parcels_in_range(focus, load_distance.load_imposter);
    for (parcel, _) in parcels {
        if let Some((hash, _)) = scene_pointers
            .0
            .get(&parcel)
            .and_then(PointerResult::hash_and_urn)
        {
            let is_live = live_scenes
                .0
                .get(&hash)
                .and_then(|scene| scene_transforms.get(*scene).ok())
                .map_or(false, |t| t.translation.y >= 0.0);
            if !is_live
                && baked_imposters
                    .scene_imposters
                    .get(&hash)
                    .map_or(false, |b| b.fully_baked)
            {
                required_imposters.insert(hash, true);
            }
        }
    }

    let existing_imposters: HashMap<_, _> = existing_imposters
        .iter()
        .map(|(e, h)| (&h.hash, (e, h)))
        .collect();
    for imposter_hash in existing_imposters.keys() {
        if !required_imposters.contains_key(*imposter_hash) {
            // despawn existing not required
            // println!("removing entire imposter for {imposter_hash}");
            // commands.entity(*entity).despawn_recursive();
            required_imposters.insert(imposter_hash.to_string(), false);
        }
    }

    // spawn/update required
    for (hash, required) in required_imposters {
        if let Some(baked) = baked_imposters.scene_imposters.get(&hash) {
            // spawn parent and children in correct positions
            let (imposter_root, regions) = existing_imposters
                .get(&hash)
                .map(|(e, h)| (*e, h.children.clone()))
                .unwrap_or_else(|| {
                    println!("spawning imposter parent for {hash}");
                    // imposter entity placeholders
                    let children = baked
                        .imposters
                        .iter()
                        .filter_map(|(_, spec)| spec.as_ref().and_then(|s| s.as_ref()))
                        .map(|spec| {
                            commands
                                .spawn((
                                    SpatialBundle {
                                        transform: Transform::from_translation(
                                            (spec.region_min + spec.region_max) * 0.5,
                                        ),
                                        ..Default::default()
                                    },
                                    NotShadowCaster,
                                    NotShadowReceiver,
                                ))
                                .id()
                        })
                        .collect::<Vec<_>>();

                    // add floor entities
                    let floors = baked
                        .imposters
                        .iter()
                        .filter(|(_, spec)| spec.is_some())
                        .map(|(parcels, _)| {
                            let region =
                                BoundRegion::new(parcels.parcel_min, parcels.parcel_max, 0);
                            let mid = region.world_midpoint().xz();
                            let size = region.world_size().xz();

                            let mut path = ipfas.ipfs_cache_path().to_owned();
                            path.push("imposters");
                            path.push("textures");
                            path.push(format!(
                                "{},{},{},{},{}-topdown.boimp",
                                hash,
                                parcels.parcel_min.x,
                                parcels.parcel_min.y,
                                parcels.parcel_max.x,
                                parcels.parcel_max.y
                            ));

                            commands
                                .spawn((
                                    MaterialMeshBundle {
                                        transform: Transform::from_translation(Vec3::new(
                                            mid.x, -0.01, mid.y,
                                        ))
                                        .with_scale(Vec3::new(
                                            size.max_element() + 2.0,
                                            1.0,
                                            size.max_element() + 2.0,
                                        )),
                                        mesh: imposter_meshes.floor.clone(),
                                        material: asset_server.load::<FloorImposter>(path),
                                        ..Default::default()
                                    },
                                    NotShadowCaster,
                                    NotShadowReceiver,
                                ))
                                .id()
                        })
                        .collect::<Vec<_>>();

                    let root = commands
                        .spawn((
                            SpatialBundle::default(),
                            ImposterScene {
                                hash: hash.clone(),
                                children: children.clone(),
                            },
                        ))
                        .push_children(&children)
                        .push_children(&floors)
                        .id();

                    (root, children)
                });

            let mut any_visible = false;
            for ((region, spec), entity) in baked
                .imposters
                .iter()
                .filter_map(|(p, spec)| {
                    spec.as_ref().and_then(|s| s.as_ref()).map(|spec| (p, spec))
                })
                .zip(regions)
            {
                let nearest_point = focus.translation().xz().clamp(
                    IVec2::new(region.parcel_min.x, -region.parcel_max.y).as_vec2() * 16.0,
                    IVec2::new(region.parcel_max.x + 1, -region.parcel_min.y + 1).as_vec2() * 16.0,
                );
                let distance = (focus.translation().xz() - nearest_point).length();
                let height = spec.region_max.y;
                let ratio = (distance - load_distance.load) / height;

                let show = required && ratio < load_distance.imposter_height_ratio;

                any_visible |= show;

                let (is_showing, is_transition, handle) = match existing_regions.get(entity) {
                    Ok((h, Some(_))) => (true, true, Some(h)),
                    Ok((h, None)) => (true, false, Some(h)),
                    Err(_) => (false, false, None),
                };

                match (show, is_showing, is_transition) {
                    (true, false, _) => {
                        println!("spawning imposter for {hash} @ {region:?}");
                        let mut scale = spec.region_max - spec.region_min;
                        scale.y = spec.scale * 2.0;

                        let mut path = ipfas.ipfs_cache_path().to_owned();
                        path.push("imposters");
                        path.push("textures");
                        path.push(format!(
                            "{},{},{},{},{}.boimp",
                            hash,
                            region.parcel_min.x,
                            region.parcel_min.y,
                            region.parcel_max.x,
                            region.parcel_max.y
                        ));

                        commands.entity(entity).insert((
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
                                                alpha: 0.0,
                                            }
                                        },
                                    ),
                                transform: Transform::from_translation(
                                    (spec.region_min + spec.region_max) * 0.5,
                                )
                                .with_scale(scale),
                                ..Default::default()
                            },
                            ImposterTransition,
                        ));
                    }
                    (true, true, true) => {
                        if let Some(mat) = imposter_assets.get_mut(handle.unwrap()) {
                            let new_alpha =
                                1.0f32.min(mat.data.alpha + time.delta_seconds() / TRANSITION_TIME);
                            mat.data.alpha = new_alpha;
                            if new_alpha == 1.0 {
                                commands.entity(entity).remove::<ImposterTransition>();
                            }
                        }
                    }
                    (true, true, false) => (),
                    (false, false, _) => (),
                    (false, true, _) => {
                        let mut despawn = true;
                        if let Some(mat) = imposter_assets.get_mut(handle.unwrap()) {
                            let new_alpha =
                                0.0f32.max(mat.data.alpha - time.delta_seconds() / TRANSITION_TIME);
                            mat.data.alpha = new_alpha;
                            despawn = new_alpha == 0.0;
                        };
                        if despawn {
                            println!("hiding imposter for {hash} @ {region:?}");
                            commands
                                .entity(entity)
                                .remove::<ImposterTransition>()
                                .remove::<Handle<Imposter>>();
                        } else {
                            any_visible = true;
                        }
                    }
                }
            }

            if !any_visible && !required {
                // despawn existing not required
                println!("removing entire imposter for {hash}");
                commands.entity(imposter_root).despawn_recursive();
            }
        }
    }
}

fn spec_path(ipfs: &IpfsAssetServer, hash: &str) -> PathBuf {
    let mut path = ipfs.ipfs_cache_path().to_owned();
    path.push("imposters");
    path.push("specs");
    path.push(format!("{hash}.json"));
    path
}

fn write_imposter(ipfs: &IpfsAssetServer, hash: &str, imposter: &BakedImposter) {
    let path = spec_path(ipfs, hash);
    let _ = std::fs::create_dir_all(path.parent().unwrap());
    if let Err(e) = std::fs::File::create(path)
        .map_err(|e| e.to_string())
        .and_then(|f| serde_json::to_writer(f, &imposter).map_err(|e| e.to_string()))
    {
        warn!("failed to write imposter spec: {e}");
    }
}

// fn debug_write_imposters(assets: Res<Assets<Imposter>>, tick: Res<FrameCount>) {
//     if tick.0 % 100 != 0 {
//         return;
//     }

//     let mut count = 0;
//     let mut memory: usize = 0;
//     let mut memory_compressed: usize = 0;
//     for (_, imposter) in assets.iter() {
//         count += 1;
//         memory += imposter.base_size as usize;
//         memory_compressed += imposter.compressed_size as usize;
//     }

//     info!(
//         "{} bytes ({} mb) over {} imposters",
//         memory,
//         memory / 1024 / 1024,
//         count
//     );
//     info!(
//         "{} bytes ({} mb) over {} imposters tho actually",
//         memory_compressed,
//         memory_compressed / 1024 / 1024,
//         count
//     );
// }
