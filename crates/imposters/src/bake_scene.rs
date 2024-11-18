// scenes are saved by entity id
// cache/imposters/scenes (/specs | /textures)

use bevy::{
    core::FrameCount, math::FloatOrd, prelude::*, render::primitives::Aabb, utils::HashSet,
};
use boimp::{
    bake::{BakeState, ImposterBakeBundle, ImposterBakeCamera},
    GridMode, ImposterBakePlugin,
};
use common::structs::PrimaryUser;
use ipfs::IpfsAssetServer;
use scene_material::{BoundRegion, SceneBound, SceneMaterial};

use scene_runner::{
    initialize_scene::{
        CurrentImposterScene, LiveScenes, PointerResult, ScenePointers, PARCEL_SIZE,
    },
    renderer_context::RendererSceneContext,
};

use crate::{
    imposter_spec::{
        scene_floor_path, scene_texture_path, write_scene_imposter, BakedScene, ImposterSpec,
    },
    render::{ImposterLookup, ImposterMissing, ImposterReady, SceneImposter},
};
pub struct DclImposterBakeScenePlugin;

const GRID_SIZE: u32 = 8;
const TILE_SIZE: u32 = 128;

impl Plugin for DclImposterBakeScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ImposterBakePlugin)
            .add_systems(Update, (pick_imposter_to_bake, make_oven, bake_imposters));
    }
}

#[derive(Component)]
pub struct ImposterOven {
    start_tick: u32,
    hash: String,
    unbaked_parcels: Vec<BoundRegion>,
    all_parcels: HashSet<IVec2>,
    baked_scene: BakedScene,
}

fn make_oven(
    current_imposter: Res<CurrentImposterScene>,
    mut commands: Commands,
    baking: Query<&ImposterOven>,
    mut scenes: Query<(&mut RendererSceneContext, &mut Transform)>,
    live_scenes: Res<LiveScenes>,
    tick: Res<FrameCount>,
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

        warn!("baking scene {:?}", hash);

        // gather regions
        let unbaked_parcels = context
            .bounds
            .iter()
            .flat_map(|r| {
                (r.parcel_min().x..=r.parcel_max().x).map(move |x| {
                    (r.parcel_min().y..=r.parcel_max().y).map(move |y| {
                        BoundRegion::new(IVec2::new(x, y), IVec2::new(x, y), r.parcel_count)
                    })
                })
            })
            .flatten()
            .collect::<Vec<_>>();

        // spawn oven
        commands.spawn(ImposterOven {
            start_tick: tick.0,
            hash: hash.clone(),
            unbaked_parcels,
            all_parcels: context.parcels.clone(),
            baked_scene: Default::default(),
        });
    }
}

fn bake_imposters(
    mut commands: Commands,
    mut current_imposter: ResMut<CurrentImposterScene>,
    mut baking: Query<(Entity, &mut ImposterOven)>,
    mut all_baking_cams: Query<(Entity, &mut ImposterBakeCamera)>,
    mut scenes: Query<(&mut RendererSceneContext, &mut Transform)>,
    live_scenes: Res<LiveScenes>,
    ipfas: IpfsAssetServer,
    tick: Res<FrameCount>,
    children: Query<&Children>,
    meshes: Query<(&GlobalTransform, &Aabb), With<Handle<Mesh>>>,
    bound_materials: Query<&Handle<SceneMaterial>>,
    mut materials: ResMut<Assets<SceneMaterial>>,
    lookup: Res<ImposterLookup>,
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
            let Some(region) = oven.unbaked_parcels.pop() else {
                warn!("no regions left");
                write_scene_imposter(&ipfas, &oven.hash, &oven.baked_scene);

                if let Ok((mut context, _)) = scenes.get_mut(current_scene_ent) {
                    context.blocked.remove("imposter_baking");
                }

                for parcel in std::mem::take(&mut oven.all_parcels).drain() {
                    if let Some(entity) = lookup.0.get(&(parcel, 0)) {
                        if let Some(mut commands) = commands.get_entity(*entity) {
                            commands.remove::<ImposterMissing>();

                            if let Some(spec) = oven.baked_scene.imposters.get(&parcel) {
                                commands.try_insert(spec.clone());
                            }
                            commands.try_insert(ImposterReady(Some(oven.hash.clone())));
                        }
                    }
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
                // oven.baked_scene.imposters.insert(region.parcel_min(), None);
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

                let path = scene_texture_path(ipfas.ipfs(), &oven.hash, region.parcel_min());
                let _ = std::fs::create_dir_all(path.parent().unwrap());
                let callback = camera.save_asset_callback(path, true);
                camera.set_callback(callback);

                commands.spawn(ImposterBakeBundle {
                    camera,
                    transform: Transform::from_translation(center + Vec3::Y * -2000.0),
                    ..Default::default()
                });

                oven.baked_scene.imposters.insert(
                    region.parcel_min(),
                    ImposterSpec {
                        scale: radius,
                        region_min: aabb.min().into(),
                        region_max: aabb.max().into(),
                    },
                );
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

            let path = scene_floor_path(ipfas.ipfs(), &oven.hash, region.parcel_min());
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

            // despawn on complete
            if all_cams_finished {
                warn!("finished baking");

                for (cam_ent, _) in all_baking_cams.iter() {
                    commands.entity(cam_ent).despawn_recursive();
                }
            }
        }
    }
}

fn pick_imposter_to_bake(
    q: Query<(&SceneImposter, &ImposterMissing)>,
    mut current_imposter_scene: ResMut<CurrentImposterScene>,
    focus: Query<&GlobalTransform, With<PrimaryUser>>,
    scene_pointers: Res<ScenePointers>,
    live_scenes: Res<LiveScenes>,
) {
    if current_imposter_scene.0.is_some() {
        return;
    }

    let Ok(focus) = focus.get_single().map(|gt| gt.translation()) else {
        return;
    };

    let mut missing = q
        .iter()
        .map(|(imposter, missing)| {
            let midpoint = (imposter.parcel * IVec2::new(1, -1)).as_vec2() * PARCEL_SIZE
                + ((1 << imposter.level) as f32 * PARCEL_SIZE) * 0.5;
            ((midpoint - focus.xz()).length_squared(), imposter, missing)
        })
        .filter(|(_, _, missing)| {
            missing
                .0
                .as_ref()
                .map_or(true, |m| !live_scenes.0.contains_key(m))
        })
        .collect::<Vec<_>>();

    missing.sort_by_key(|(dist, ..)| FloatOrd(*dist));

    if let Some((_, imposter, _)) = missing.first() {
        if imposter.level == 0 {
            current_imposter_scene.0 = scene_pointers.0.get(&imposter.parcel).cloned();
        } else {
            println!("too big!");
        }
    }
}
