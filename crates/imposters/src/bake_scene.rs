// scenes are saved by entity id
// cache/imposters/scenes (/specs | /textures)

use bevy::{
    core::FrameCount,
    math::FloatOrd,
    prelude::*,
    render::{primitives::Aabb, view::RenderLayers},
    utils::HashSet,
};
use boimp::{
    bake::{BakeState, ImposterBakeBundle, ImposterBakeCamera},
    GridMode, ImposterBakePlugin,
};
use common::{
    sets::SceneSets,
    structs::{AppConfig, PrimaryUser, SceneImposterBake},
};
use crc::CRC_32_CKSUM;
use ipfs::{CurrentRealm, IpfsAssetServer};
use scene_material::{BoundRegion, SceneBound, SceneMaterial};

use scene_runner::{
    initialize_scene::{
        CurrentImposterScene, LiveScenes, PointerResult, ScenePointers, PARCEL_SIZE,
    },
    renderer_context::RendererSceneContext,
    DebugInfo,
};

use crate::{
    imposter_spec::{floor_path, texture_path, write_imposter, BakedScene, ImposterSpec},
    render::{
        BakingIngredients, ImposterEntities, ImposterLookup, ImposterMissing, ImposterReady,
        ImposterState, ImposterTransitionOut, SceneImposter,
    },
};
pub struct DclImposterBakeScenePlugin;

const GRID_SIZE: u32 = 9;
const TILE_SIZE: u32 = 96;

pub const IMPOSTERCEPTION_LAYER: RenderLayers = RenderLayers::layer(5);

impl Plugin for DclImposterBakeScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ImposterBakePlugin)
            .init_resource::<ImposterBakeList>()
            .init_resource::<CurrentImposterImposter>()
            .add_systems(
                Update,
                (
                    make_scene_oven,
                    bake_scene_imposters,
                    bake_imposter_imposter,
                    check_bake_state,
                    pick_imposter_to_bake,
                )
                    .chain()
                    .before(crate::render::spawn_imposters)
                    .before(SceneSets::UiActions),
            );
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

const SCENE_BAKE_TICK: u32 = 100;

fn make_scene_oven(
    current_imposter: Res<CurrentImposterScene>,
    mut commands: Commands,
    baking: Query<&ImposterOven>,
    mut scenes: Query<(&mut RendererSceneContext, &mut Transform)>,
    live_scenes: Res<LiveScenes>,
    tick: Res<FrameCount>,
    mut start_tick: Local<Option<(String, u32)>>,
    bake_list: Res<ImposterBakeList>,
    children: Query<&Children>,
    mat_handles: Query<&Handle<SceneMaterial>>,
    materials: Res<Assets<SceneMaterial>>,
    asset_server: Res<AssetServer>,
) {
    if !baking.is_empty() {
        return;
    }

    let Some((PointerResult::Exists { hash, .. }, false)) = &current_imposter.0 else {
        return;
    };

    if start_tick
        .as_ref()
        .is_none_or(|(start_tick_hash, _)| hash != start_tick_hash)
    {
        *start_tick = Some((hash.clone(), tick.0));
    }

    if let Some(entity) = live_scenes.0.get(hash) {
        let Ok((mut context, mut transform)) = scenes.get_mut(*entity) else {
            if tick.0 > start_tick.as_ref().unwrap().1 + 1000 {
                warn!("scene load failed, spawning dummy oven");
                let Some(ImposterToBake::Scene(parcel, _)) = bake_list.0.last() else {
                    error!("bake list doesn't have a scene, scene entity is missing, but CurrentImposterScene is set?!");
                    return;
                };
                commands.spawn(ImposterOven {
                    start_tick: tick.0,
                    hash: hash.clone(),
                    unbaked_parcels: vec![BoundRegion::new(*parcel, *parcel, 1)],
                    all_parcels: HashSet::from_iter([*parcel]),
                    baked_scene: BakedScene {
                        crc: crc::Crc::<u32>::new(&CRC_32_CKSUM).checksum(hash.as_bytes()),
                        ..Default::default()
                    },
                });
            }
            return;
        };

        transform.translation.y = -2000.0;

        let spawning_forever = tick.0 > start_tick.as_ref().unwrap().1 + 1000;

        if context.tick_number < SCENE_BAKE_TICK && !context.broken && !spawning_forever {
            return;
        }

        // check for texture loads
        if !spawning_forever {
            for child in children.iter_descendants(*entity) {
                if let Ok(h_mat) = mat_handles.get(child) {
                    let Some(mat) = materials.get(h_mat.id()) else {
                        return;
                    };
                    for h_texture in [
                        &mat.base.base_color_texture,
                        &mat.base.normal_map_texture,
                        &mat.base.metallic_roughness_texture,
                        &mat.base.emissive_texture,
                    ]
                    .into_iter()
                    .flatten()
                    {
                        if matches!(
                            asset_server.load_state(h_texture.id()),
                            bevy::asset::LoadState::Loading
                        ) {
                            return;
                        }
                    }
                }
            }
        }

        context.blocked.insert("imposter_baking");
        if context.in_flight && !context.broken && !spawning_forever {
            return;
        }

        warn!("baking scene {:?}", hash);

        // disable animations and tweens
        for child in children.iter_descendants(*entity) {
            if let Some(mut commands) = commands.get_entity(child) {
                commands
                    .remove::<AnimationPlayer>()
                    .remove::<tween::Tween>();
            }
        }

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
            baked_scene: BakedScene {
                crc: crc::Crc::<u32>::new(&CRC_32_CKSUM).checksum(hash.as_bytes()),
                ..Default::default()
            },
        });
    }
}

fn bake_scene_imposters(
    mut commands: Commands,
    mut current_imposter: ResMut<CurrentImposterScene>,
    mut baking: Query<(Entity, &mut ImposterOven)>,
    mut all_baking_cams: Query<(Entity, &mut ImposterBakeCamera)>,
    mut live_scenes: ResMut<LiveScenes>,
    ipfas: IpfsAssetServer,
    tick: Res<FrameCount>,
    children: Query<&Children>,
    meshes: Query<(&GlobalTransform, &Aabb, &Visibility), With<Handle<Mesh>>>,
    bound_materials: Query<&Handle<SceneMaterial>>,
    mut materials: ResMut<Assets<SceneMaterial>>,
    lookup: Res<ImposterEntities>,
    config: Res<AppConfig>,
) {
    if let Ok((baking_ent, mut oven)) = baking.get_single_mut() {
        let current_scene_ent = {
            let Some(entity) = live_scenes.0.get(&oven.hash) else {
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
                write_imposter(&ipfas, &oven.hash, IVec2::MAX, 0, &oven.baked_scene);

                // delete the scene since we messed with it a lot to get it stable
                commands.entity(current_scene_ent).despawn_recursive();
                live_scenes.0.remove(&oven.hash);

                for parcel in std::mem::take(&mut oven.all_parcels).drain() {
                    for ingredient in [true, false] {
                        if let Some(entity) = lookup.0.get(&(parcel, 0, ingredient)) {
                            if let Some(mut commands) = commands.get_entity(*entity) {
                                commands.remove::<ImposterMissing>();

                                if let Some(spec) = oven.baked_scene.imposters.get(&parcel) {
                                    commands.try_insert(spec.clone());
                                }
                                commands.try_insert(ImposterReady {
                                    scene: Some(oven.hash.clone()),
                                    crc: oven.baked_scene.crc,
                                });
                            }
                        }
                    }
                }

                current_imposter.0.as_mut().unwrap().1 = true;
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
                mat.extension.data = SceneBound::new(vec![region], 1.0).data;
            }

            // region bounds
            let rmin = region.world_min();
            let rmax = region.world_max();

            // content bounds
            let mut points = Vec::default();
            for (gt, aabb, vis) in children
                .iter_descendants(current_scene_ent)
                .filter_map(|e| meshes.get(e).ok())
            {
                if matches!(vis, Visibility::Hidden) {
                    continue;
                };
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
                let intersects = (max + Vec3::Y * 2000.0).cmpge(rmin).all()
                    && (min + Vec3::Y * 2000.0).cmple(rmax).all();
                if max.y > -1999.0 /* && max.y > min.y */ && intersects {
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
                    .clamp(2.0, TILE_SIZE as f32 * 2.0) as u32;
                warn!("tile size: {tile_size}");

                let max_tiles_per_frame = ((GRID_SIZE * GRID_SIZE) as f32
                    * config.scene_imposter_bake.as_mult())
                .ceil() as usize;

                let mut camera = ImposterBakeCamera {
                    radius,
                    grid_size: GRID_SIZE,
                    tile_size,
                    grid_mode: GridMode::Hemispherical,
                    max_tiles_per_frame,
                    ..Default::default()
                };

                let path = texture_path(ipfas.ipfs(), &oven.hash, region.parcel_min(), 0);
                let _ = std::fs::create_dir_all(path.parent().unwrap());
                let callback = camera.save_asset_callback(path, true, true);
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

            let path = floor_path(ipfas.ipfs(), &oven.hash, region.parcel_min(), 0);
            let callback = top_down.save_asset_callback(path, true, true);
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
            if oven.start_tick + 200 < tick.0 {
                for (_, mut cam) in all_baking_cams.iter_mut() {
                    if cam.wait_for_render {
                        warn!("forcing failed bake ...");
                        cam.wait_for_render = false;
                    }
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

fn bake_imposter_imposter(
    mut commands: Commands,
    mut current_imposter: ResMut<CurrentImposterImposter>,
    existing_imposters: Query<(Option<&ImposterSpec>, &Children)>,
    mut all_baking_cams: Query<(Entity, &mut ImposterBakeCamera)>,
    lookup: Res<ImposterEntities>,
    mut baking: Local<Option<(u32, BakedScene)>>,
    ipfas: IpfsAssetServer,
    current_realm: Res<CurrentRealm>,
    mut layers: Query<&mut RenderLayers>,
    tick: Res<FrameCount>,
    config: Res<AppConfig>,
) {
    if baking.is_some() {
        let all_cams_finished = all_baking_cams
            .iter()
            .all(|(_, cam)| cam.state == BakeState::Finished);

        if all_cams_finished {
            let (_, baking) = baking.take().unwrap();
            warn!("all cams done");
            let Some(CurrentImposterImposterDetail { parcel, level, .. }) = current_imposter.0
            else {
                return;
            };

            for ingredient in [true, false] {
                if let Some(entity) = lookup.0.get(&(parcel, level, ingredient)) {
                    if let Some(mut commands) = commands.get_entity(*entity) {
                        commands.remove::<ImposterMissing>();

                        if let Some(spec) = baking.imposters.get(&parcel) {
                            commands.try_insert(spec.clone());
                        }
                        commands.try_insert(ImposterReady {
                            scene: None,
                            crc: baking.crc,
                        });
                    }
                }
            }

            for (ent, _) in all_baking_cams.iter() {
                if let Some(commands) = commands.get_entity(ent) {
                    commands.despawn_recursive();
                }
            }

            write_imposter(&ipfas, &current_realm.address, parcel, level, &baking);
            current_imposter.0.as_mut().unwrap().complete = true;
        } else if tick.0 > baking.as_ref().unwrap().0 + 100 {
            for (_, mut cam) in all_baking_cams.iter_mut() {
                warn!("forcing failed imposter bake ...");
                cam.wait_for_render = false;
            }
        }

        return;
    }

    if let Some(CurrentImposterImposterDetail {
        parcel,
        level,
        complete: false,
        crc,
    }) = current_imposter.0.as_ref()
    {
        warn!("baking mip: {:?}-{}", parcel, level);
        let size = 1 << level;
        let next_size = 1 << (level - 1);
        let mut baked_scene = BakedScene {
            crc: *crc,
            ..Default::default()
        };

        let mut min = Vec3::MAX;
        let mut max = Vec3::MIN;
        for offset in [IVec2::ZERO, IVec2::X, IVec2::Y, IVec2::ONE] {
            let key = (*parcel + offset * next_size, level - 1, true);

            if let Some((maybe_spec, children)) = lookup
                .0
                .get(&key)
                .and_then(|e| existing_imposters.get(*e).ok())
            {
                if let Some(spec) = maybe_spec {
                    min = min.min(spec.region_min);
                    max = max.max(spec.region_max);
                }

                // add layer to children
                for child in children.iter() {
                    let mut layer = layers.get_mut(*child).unwrap();
                    *layer = layer.union(&IMPOSTERCEPTION_LAYER);
                }
            }
        }

        // skip empty
        if min.cmpgt(max).any() {
            warn!("skipping empty imposterception bake");
            // oven.baked_scene.imposters.insert(region.parcel_min(), None);
        } else {
            let aabb = Aabb::from_min_max(min, max);
            let center = Vec3::from(aabb.center);
            let radius = aabb.half_extents.length();

            let tile_size = ((TILE_SIZE / size) as f32
                * aabb.half_extents.xz().length().max(aabb.half_extents.y)
                / 16.0)
                .clamp(2.0, 256.0) as u32;
            warn!("tile size: {tile_size}");

            let max_tiles_per_frame = ((GRID_SIZE * GRID_SIZE) as f32
                * config.scene_imposter_bake.as_mult())
            .ceil() as usize;

            let mut camera = ImposterBakeCamera {
                radius,
                grid_size: GRID_SIZE,
                tile_size,
                grid_mode: GridMode::Hemispherical,
                max_tiles_per_frame,
                multisample: 8,
                ..Default::default()
            };

            let path = texture_path(ipfas.ipfs(), &current_realm.address, *parcel, *level);
            let _ = std::fs::create_dir_all(path.parent().unwrap());
            let callback = camera.save_asset_callback(path, true, true);
            camera.set_callback(callback);

            commands.spawn((
                ImposterBakeBundle {
                    camera,
                    transform: Transform::from_translation(center),
                    ..Default::default()
                },
                IMPOSTERCEPTION_LAYER,
            ));

            baked_scene.imposters.insert(
                *parcel,
                ImposterSpec {
                    scale: radius,
                    region_min: aabb.min().into(),
                    region_max: aabb.max().into(),
                },
            );
        }

        // generate top-down unless crc == 0
        if *crc != 0 {
            let mid = (*parcel * IVec2::new(1, -1) * 16).as_vec2()
                + Vec2::new(size as f32, -(size as f32)) * 16.0 * 0.5;
            let mut top_down = ImposterBakeCamera {
                grid_size: 1,
                radius: (size as f32) * PARCEL_SIZE * 0.5 + size as f32,
                tile_size: 16 + 2,
                order: -98,
                multisample: 1,
                manual_camera_transforms: Some(vec![Transform::from_translation(Vec3::new(
                    mid.x, 2.0, mid.y,
                ))
                .looking_at(Vec3::new(mid.x, 0.0, mid.y), Vec3::NEG_Z)
                .into()]),
                ..Default::default()
            };

            let path = floor_path(ipfas.ipfs(), &current_realm.address, *parcel, *level);
            let callback = top_down.save_asset_callback(path, true, true);
            top_down.set_callback(callback);

            commands.spawn((
                ImposterBakeBundle {
                    transform: Transform::from_translation(Vec3::new(mid.x, 0.0, mid.y)),
                    camera: top_down,
                    ..Default::default()
                },
                IMPOSTERCEPTION_LAYER,
            ));
        }

        *baking = Some((tick.0, baked_scene));
    }
}

#[derive(Debug)]
pub enum ImposterToBake {
    Scene(IVec2, PointerResult),
    Mip(IVec2, usize),
}

#[derive(Resource, Default, Debug)]
pub struct ImposterBakeList(Vec<ImposterToBake>);

fn pick_imposter_to_bake(
    q: Query<(&SceneImposter, &ImposterMissing), Without<ImposterTransitionOut>>,
    focus: Query<&GlobalTransform, With<PrimaryUser>>,
    scene_pointers: Res<ScenePointers>,
    live_scenes: Res<LiveScenes>,
    mut baking: ResMut<ImposterBakeList>,
    current_realm: Res<CurrentRealm>,
    config: Res<AppConfig>,
) {
    if config.scene_imposter_bake == SceneImposterBake::Off {
        return;
    }

    if current_realm.is_changed() {
        baking.0.clear();
        return;
    }

    if current_realm.address.is_empty() {
        return;
    }

    if !baking.0.is_empty() {
        return;
    }

    let focus = focus
        .get_single()
        .map(|gt| gt.translation())
        .unwrap_or_default();

    let mut missing = q
        .iter()
        .map(|(imposter, missing)| {
            let midpoint = (imposter.parcel.as_vec2() + ((1 << imposter.level) as f32) * 0.5)
                * Vec2::new(1.0, -1.0)
                * PARCEL_SIZE;
            ((midpoint - focus.xz()).length_squared(), imposter, missing)
        })
        .filter(|(_, imposter, missing)| {
            missing
                .0
                .as_ref()
                .map_or(imposter.level > 0, |m| !live_scenes.0.contains_key(m))
        })
        .collect::<Vec<_>>();

    missing.sort_by_key(|(dist, ..)| FloatOrd(*dist));

    'imposter: for (_, imposter, _) in missing.into_iter() {
        if imposter.level == 0 {
            if let Some(pointer) = scene_pointers.get(imposter.parcel).cloned() {
                if matches!(pointer, PointerResult::Exists { .. }) {
                    info!("baking picked {imposter:?}");
                    baking
                        .0
                        .push(ImposterToBake::Scene(imposter.parcel, pointer));
                    break;
                }
            }
        } else {
            // check all scenes in the parcel
            let size = 1 << imposter.level;
            for x in imposter.parcel.x..imposter.parcel.x + size {
                for y in imposter.parcel.y..imposter.parcel.y + size {
                    if let Some(PointerResult::Exists { hash, .. }) =
                        scene_pointers.get(IVec2::new(x, y))
                    {
                        if live_scenes.0.get(hash).is_some() {
                            // skip due to live scene
                            continue 'imposter;
                        }
                    }
                }
            }

            info!("baking picked {imposter:?}");
            baking
                .0
                .push(ImposterToBake::Mip(imposter.parcel, imposter.level));
            break;
        }
    }
}

#[derive(Resource, Default, Debug)]
pub struct CurrentImposterImposter(Option<CurrentImposterImposterDetail>);

#[derive(Debug)]
pub struct CurrentImposterImposterDetail {
    parcel: IVec2,
    level: usize,
    complete: bool,
    crc: u32,
}

fn check_bake_state(
    mut baking: ResMut<ImposterBakeList>,
    mut current_imposter_scene: ResMut<CurrentImposterScene>,
    mut current_imposter_imposter: ResMut<CurrentImposterImposter>,
    mut ingredients: ResMut<BakingIngredients>,
    lookup: ImposterLookup,
    mut scene_pointers: ResMut<ScenePointers>,
    mut debug_info: ResMut<DebugInfo>,
) {
    if !baking.0.is_empty() {
        debug_info.info.insert(
            "Imposter Generation",
            format!(
                "{:?}",
                baking
                    .0
                    .iter()
                    .map(|p| format!("{p:?}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
        );
    } else {
        debug_info.info.remove("Imposter Generation");
    }

    ingredients.0.clear();

    if let Some(imposter) = baking.0.last() {
        debug!("bake: {:?}", imposter);
        match imposter {
            ImposterToBake::Scene(parcel, pointer_result) => {
                if current_imposter_scene
                    .0
                    .as_ref()
                    .is_some_and(|(_, done)| *done)
                {
                    // done
                    info!("scene done!");
                    baking.0.pop();
                    current_imposter_scene.0 = None;
                } else {
                    // don't need to check for constituents, just go
                    debug!("scene running!");
                    if current_imposter_scene.0.is_none() {
                        current_imposter_scene.0 = Some((pointer_result.clone(), false));
                        ingredients.0.push((*parcel, 0));
                    }
                }
            }
            ImposterToBake::Mip(parcel, level) => {
                if current_imposter_imposter
                    .0
                    .as_ref()
                    .is_some_and(|detail| detail.complete)
                {
                    info!("mip done!");
                    current_imposter_imposter.0 = None;
                    baking.0.clear();
                    return;
                } else {
                    debug!("mip bake state: {:?}", lookup.state(*parcel, *level, true));
                }

                let next_size = 1 << (level - 1);
                let min = (scene_pointers.min() >> (level - 1) as u32) << (level - 1) as u32;
                let max = (scene_pointers.max() >> (level - 1) as u32) << (level - 1) as u32;

                if ingredients.0.last() != Some(&(*parcel, *level)) {
                    for offset in [IVec2::ZERO, IVec2::X, IVec2::Y, IVec2::ONE] {
                        let key = (*parcel + offset * next_size, level - 1);
                        if key.0.clamp(min, max) != key.0 {
                            continue;
                        }
                        ingredients.0.push(key);
                    }
                    ingredients.0.push((*parcel, *level));
                }

                let mut any_pending = false;
                for offset in [IVec2::ZERO, IVec2::X, IVec2::Y, IVec2::ONE] {
                    let key = (*parcel + offset * next_size, level - 1);
                    let mut pointer = None;
                    if *level == 1 {
                        match scene_pointers.get(key.0) {
                            Some(p @ PointerResult::Exists { .. }) => pointer = Some(p),
                            Some(PointerResult::Nothing) => continue,
                            None => {
                                error!("missing scene pointer for {}, bailing", &key.0);
                                ingredients.0.clear();
                                baking.0.clear();
                                return;
                            }
                        }
                    } else if key.0.clamp(min, max) != key.0 {
                        continue;
                    }

                    match lookup.state(key.0, key.1, true) {
                        ImposterState::Ready | ImposterState::NoScene => (),
                        ImposterState::Missing => {
                            if *level == 1 {
                                baking
                                    .0
                                    .push(ImposterToBake::Scene(key.0, pointer.unwrap().clone()));
                            } else {
                                baking.0.push(ImposterToBake::Mip(key.0, key.1));
                            }
                            return;
                        }
                        _ => {
                            debug!("waiting for pending {key:?}");
                            any_pending = true;
                        }
                    }
                }

                if any_pending {
                    return;
                }

                let crc = scene_pointers.crc(parcel, *level).unwrap();

                // run the bake
                current_imposter_imposter.0 = Some(CurrentImposterImposterDetail {
                    parcel: *parcel,
                    level: *level,
                    complete: false,
                    crc,
                });
            }
        }
    }
}
