// scenes are saved by entity id
// cache/imposters/scenes (/specs | /textures)

use std::{
    io::{Read, Write},
    path::PathBuf,
    time::Duration,
};

use bevy::{
    diagnostic::FrameCount,
    math::FloatOrd,
    prelude::*,
    render::{primitives::Aabb, view::RenderLayers},
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
use zip::{write::SimpleFileOptions, ZipWriter};

use crate::{
    imposter_spec::{
        floor_path, spec_path, texture_path, write_imposter, zip_path, BakedScene, ImposterSpec,
    },
    render::{
        BakingIngredients, ImposterSpecManager, ImposterState, RetryImposter, SceneImposter,
        SpecStateReady,
    },
    DclImposterPlugin,
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
                    output_progress,
                )
                    .chain()
                    .in_set(SceneSets::PostInit),
            );
    }
}

#[derive(Component)]
pub struct ImposterOven {
    start_tick: u32,
    hash: String,
    unbaked_parcels: Vec<BoundRegion>,
    baked_scene: BakedScene,
}

const SCENE_BAKE_TICK: u32 = 200;

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
    mat_handles: Query<&MeshMaterial3d<SceneMaterial>>,
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

    if let Some(entity) = live_scenes.scenes.get(hash) {
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
                    baked_scene: BakedScene {
                        crc: crc::Crc::<u32>::new(&CRC_32_CKSUM).checksum(hash.as_bytes()),
                        ..Default::default()
                    },
                });
            }
            return;
        };

        transform.translation.y = -2000.0;

        let spawning_forever = tick.0 > start_tick.as_ref().unwrap().1 + 10000;

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

        debug!("baking scene {:?}", hash);

        // disable animations and tweens
        for child in children.iter_descendants(*entity) {
            if let Ok(mut commands) = commands.get_entity(child) {
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
    meshes: Query<(&GlobalTransform, &Aabb, &Visibility), With<Mesh3d>>,
    bound_materials: Query<&MeshMaterial3d<SceneMaterial>>,
    mut materials: ResMut<Assets<SceneMaterial>>,
    config: Res<AppConfig>,
    plugin: Res<DclImposterPlugin>,
) {
    if let Ok((baking_ent, mut oven)) = baking.single_mut() {
        let current_scene_ent = {
            let Some(entity) = live_scenes.scenes.get(&oven.hash) else {
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
                debug!("no regions left");
                write_imposter(
                    ipfas.ipfs_cache_path(),
                    &oven.hash,
                    IVec2::MAX,
                    0,
                    &oven.baked_scene,
                );
                if let Some(output_path) = plugin.zip_output.clone() {
                    save_and_zip_callback::<()>(
                        |_| {},
                        spec_path(ipfas.ipfs_cache_path(), &oven.hash, IVec2::MAX, 0),
                        output_path,
                        oven.hash.clone(),
                        IVec2::MAX,
                        0,
                        None,
                    )(());
                }

                // delete the scene since we messed with it a lot to get it stable
                commands.entity(current_scene_ent).despawn();
                live_scenes.scenes.remove(&oven.hash);

                current_imposter.0.as_mut().unwrap().1 = true;
                commands.entity(baking_ent).despawn();
                return;
            };

            debug!("baking region: {:?}", region);

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

                debug!("region: {rmin}-{rmax}, snap: {}-{}", aabb.min(), aabb.max());
                let tile_size = (TILE_SIZE as f32
                    * aabb.half_extents.xz().length().max(aabb.half_extents.y)
                    / 16.0)
                    .clamp(2.0, TILE_SIZE as f32 * 2.0) as u32;
                debug!("tile size: {tile_size}");

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

                let path =
                    texture_path(ipfas.ipfs_cache_path(), &oven.hash, region.parcel_min(), 0);
                let _ = std::fs::create_dir_all(path.parent().unwrap());
                let save_asset_callback = camera.save_asset_callback(&path, true, true);

                if let Some(output_path) = plugin.zip_output.clone() {
                    camera.set_callback(save_and_zip_callback(
                        save_asset_callback,
                        path,
                        output_path,
                        oven.hash.clone(),
                        region.parcel_min(),
                        0,
                        None,
                    ));
                } else {
                    camera.set_callback(save_asset_callback);
                }

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

            let path = floor_path(ipfas.ipfs_cache_path(), &oven.hash, region.parcel_min(), 0);
            let save_asset_callback = top_down.save_asset_callback(&path, true, true);

            if let Some(output_path) = plugin.zip_output.clone() {
                top_down.set_callback(save_and_zip_callback(
                    save_asset_callback,
                    path,
                    output_path,
                    oven.hash.clone(),
                    region.parcel_min(),
                    0,
                    None,
                ));
            } else {
                top_down.set_callback(save_asset_callback);
            }

            commands.spawn(ImposterBakeBundle {
                transform: Transform::from_translation(Vec3::new(tile_mid.x, -2000.0, tile_mid.z)),
                camera: top_down,
                ..Default::default()
            });
        } else {
            if tick.0.is_multiple_of(200) {
                debug!("waiting for bake ...");
            }

            // force failed
            if oven.start_tick + 200 < tick.0 {
                for (_, mut cam) in all_baking_cams.iter_mut() {
                    if cam.wait_for_render {
                        debug!("forcing failed bake ...");
                        cam.wait_for_render = false;
                    }
                }
            }

            // despawn on complete
            if all_cams_finished {
                debug!("finished baking");

                for (cam_ent, _) in all_baking_cams.iter() {
                    commands.entity(cam_ent).despawn();
                }
            }
        }
    }
}

fn save_and_zip_callback<T>(
    save_asset_callback: impl FnOnce(T) + Send + Sync + 'static,
    path: PathBuf,
    zip: PathBuf,
    id: String,
    parcel: IVec2,
    level: usize,
    crc: Option<u32>,
) -> impl FnOnce(T) + Send + Sync + 'static {
    move |arg: T| {
        save_asset_callback(arg);

        let output_path = zip_path(Some(&zip), &id, parcel, level, crc);
        let target_file = path.file_name().unwrap().to_string_lossy().into_owned();

        // create folder if required
        std::fs::create_dir_all(output_path.parent().unwrap()).unwrap();

        // lock
        let touch = output_path.clone().with_extension("touch");
        while std::fs::File::create_new(&touch).is_err() {
            // wait
            std::thread::sleep(Duration::ZERO);
        }

        // move and read prev version
        let mut old_path = output_path.clone();
        old_path.set_extension(".old");
        let prev_archive = if std::fs::exists(&output_path).unwrap() {
            std::fs::rename(&output_path, &old_path).unwrap();
            let old_file = std::fs::File::open(&old_path).unwrap();
            Some(zip::ZipArchive::new(old_file).unwrap())
        } else {
            None
        };

        // create new
        let output = std::fs::File::create_new(&output_path).unwrap();
        let mut archive = ZipWriter::new(output);

        // copy contents (except current file)
        if let Some(mut prev_archive) = prev_archive {
            let file_names = prev_archive
                .file_names()
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>();
            for file in file_names {
                if file != target_file {
                    archive
                        .raw_copy_file(prev_archive.by_name(&file).unwrap())
                        .unwrap();
                }
            }

            // delete prev
            std::fs::remove_file(&old_path).unwrap();
        }

        // write
        let mut file = std::fs::File::open(&path).unwrap();
        let mut data = Vec::default();
        file.read_to_end(&mut data).unwrap();
        archive
            .start_file(
                &target_file,
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        archive.write_all(&data).unwrap();
        archive.finish().unwrap();
        debug!("added {target_file} to {output_path:?}");

        // unlock
        std::fs::remove_file(touch).unwrap();
    }
}

fn bake_imposter_imposter(
    mut commands: Commands,
    mut current_imposter: ResMut<CurrentImposterImposter>,
    mut all_baking_cams: Query<(Entity, &mut ImposterBakeCamera)>,
    mut baking: Local<Option<(u32, BakedScene)>>,
    ipfas: IpfsAssetServer,
    current_realm: Res<CurrentRealm>,
    tick: Res<FrameCount>,
    config: Res<AppConfig>,
    plugin: Res<DclImposterPlugin>,
    mut manager: ImposterSpecManager,
) {
    if baking.is_some() {
        let all_cams_finished = all_baking_cams
            .iter()
            .all(|(_, cam)| cam.state == BakeState::Finished);

        if all_cams_finished {
            let (_, baking) = baking.take().unwrap();
            debug!("all cams done");
            let Some(CurrentImposterImposterDetail { parcel, level, .. }) = current_imposter.0
            else {
                return;
            };

            for (ent, _) in all_baking_cams.iter() {
                if let Ok(mut commands) = commands.get_entity(ent) {
                    commands.despawn();
                }
            }

            write_imposter(
                ipfas.ipfs_cache_path(),
                &current_realm.about_url,
                parcel,
                level,
                &baking,
            );
            if let Some(output_path) = plugin.zip_output.clone() {
                save_and_zip_callback::<()>(
                    |_| {},
                    spec_path(
                        ipfas.ipfs_cache_path(),
                        &current_realm.about_url,
                        parcel,
                        level,
                    ),
                    output_path,
                    current_realm.about_url.clone(),
                    parcel,
                    level,
                    Some(baking.crc),
                )(());
            }
            current_imposter.0.as_mut().unwrap().complete = true;
        } else if tick.0 > baking.as_ref().unwrap().0 + 100 {
            for (_, mut cam) in all_baking_cams.iter_mut() {
                debug!("forcing failed imposter bake ...");
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
        debug!("baking mip: {:?}-{}", parcel, level);
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

            let spec_state = manager.get_spec(&SceneImposter {
                parcel: key.0,
                level: key.1,
                as_ingredient: false,
            });
            match spec_state {
                crate::render::ImposterSpecState::Ready(SpecStateReady {
                    imposter_data: Some((imposter_spec, _)),
                    ..
                }) => {
                    min = min.min(imposter_spec.region_min);
                    max = max.max(imposter_spec.region_max);
                }
                crate::render::ImposterSpecState::Pending => panic!(),
                _ => continue,
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
            debug!("tile size: {tile_size}");

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

            let path = texture_path(
                ipfas.ipfs_cache_path(),
                &current_realm.about_url,
                *parcel,
                *level,
            );
            let _ = std::fs::create_dir_all(path.parent().unwrap());
            let save_asset_callback = camera.save_asset_callback(&path, true, true);

            if let Some(output_path) = plugin.zip_output.clone() {
                camera.set_callback(save_and_zip_callback(
                    save_asset_callback,
                    path,
                    output_path,
                    current_realm.about_url.clone(),
                    *parcel,
                    *level,
                    Some(*crc),
                ));
            } else {
                camera.set_callback(save_asset_callback);
            }

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

            let path = floor_path(
                ipfas.ipfs_cache_path(),
                &current_realm.about_url,
                *parcel,
                *level,
            );
            let save_asset_callback = top_down.save_asset_callback(&path, true, true);

            if let Some(output_path) = plugin.zip_output.clone() {
                top_down.set_callback(save_and_zip_callback(
                    save_asset_callback,
                    path,
                    output_path,
                    current_realm.about_url.clone(),
                    *parcel,
                    *level,
                    Some(*crc),
                ));
            } else {
                top_down.set_callback(save_asset_callback);
            }

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
    q: Query<&SceneImposter, (Without<RetryImposter>, Without<Children>)>,
    focus: Query<&GlobalTransform, With<PrimaryUser>>,
    mut scene_pointers: ResMut<ScenePointers>,
    live_scenes: Res<LiveScenes>,
    mut baking: ResMut<ImposterBakeList>,
    current_realm: Res<CurrentRealm>,
    config: Res<AppConfig>,
    plugin: Res<DclImposterPlugin>,
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
        .single()
        .map(|gt| gt.translation())
        .unwrap_or_default();

    let mut missing = q
        .iter()
        .map(|imposter| {
            let midpoint = (imposter.parcel.as_vec2() + ((1 << imposter.level) as f32) * 0.5)
                * Vec2::new(1.0, -1.0)
                * PARCEL_SIZE;
            ((midpoint - focus.xz()).length_squared(), imposter)
        })
        .filter(|(_, imposter)| {
            imposter.level > 0 || {
                let Some(PointerResult::Exists { hash, .. }) = scene_pointers.get(imposter.parcel)
                else {
                    return false;
                };
                let is_live = live_scenes.scenes.contains_key(hash);
                !is_live
            }
        })
        .collect::<Vec<_>>();

    missing.sort_by_key(|(dist, ..)| FloatOrd(*dist));

    'imposter: for (_, imposter) in missing.into_iter() {
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
                        if live_scenes.scenes.get(hash).is_some() {
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

            // delete prev zip if exists
            if let Some(output_path) = plugin.zip_output.clone() {
                let path = zip_path(
                    Some(&output_path),
                    &current_realm.about_url,
                    imposter.parcel,
                    imposter.level,
                    scene_pointers.crc(imposter.parcel, imposter.level),
                );
                let _ = std::fs::remove_file(path);
            }

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
    mut manager: ImposterSpecManager,
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
                    current_imposter_scene.0 = None;
                    let PointerResult::Exists { hash, .. } = pointer_result else {
                        panic!()
                    };
                    manager.clear_scene(hash);
                    baking.0.pop();
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
                    let crc = manager.pointers.crc(parcel, *level).unwrap();
                    manager.clear_mip(*parcel, *level, crc);
                    current_imposter_imposter.0 = None;
                    baking.0.clear();
                    return;
                }

                let next_size = 1 << (level - 1);
                let min = (manager.pointers.min() >> (level - 1) as u32) << (level - 1) as u32;
                let max = (manager.pointers.max() >> (level - 1) as u32) << (level - 1) as u32;

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
                        match manager.pointers.get(key.0) {
                            Some(p @ PointerResult::Exists { .. }) => pointer = Some(p.clone()),
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

                    match manager.get_imposter(&SceneImposter {
                        parcel: key.0,
                        level: key.1,
                        as_ingredient: true,
                    }) {
                        ImposterState::Ready(..) => (),
                        ImposterState::Missing => {
                            if *level == 1 {
                                baking
                                    .0
                                    .push(ImposterToBake::Scene(key.0, pointer.unwrap()));
                            } else {
                                baking.0.push(ImposterToBake::Mip(key.0, key.1));
                            }
                            return;
                        }
                        other => {
                            debug!("waiting for pending {key:?} ({other:?})");
                            any_pending = true;
                        }
                    }
                }

                if any_pending {
                    return;
                }

                let crc = manager.pointers.crc(parcel, *level).unwrap();

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

fn output_progress(
    q: Query<&SceneImposter, (Without<Children>, Without<RetryImposter>)>,
    mut max_count: Local<usize>,
    config: Res<AppConfig>,
    time: Res<Time>,
    mut last_time: Local<f32>,
    plugin: Res<DclImposterPlugin>,
) {
    if plugin.zip_output.is_none() {
        return;
    }

    if time.elapsed_secs() < *last_time + 5.0 {
        return;
    }
    *last_time = time.elapsed_secs();

    let max_level = config.scene_imposter_distances.len() - 1;
    let count = q.iter().filter(|i| i.level == max_level).count();
    *max_count = count.max(*max_count);
    info!(
        "imposter bake progress: {}/{} [l{max_level}] = {}%",
        *max_count - count,
        *max_count,
        ((*max_count - count) as f32 / *max_count as f32 * 100.0).floor() as usize
    );
}
