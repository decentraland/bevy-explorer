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
    platform::collections::HashMap,
    prelude::*,
    render::{primitives::Aabb, view::RenderLayers},
};
use boimp::{
    bake::{BakeState, ImposterBakeBundle, ImposterBakeCamera},
    GridMode, ImposterBakePlugin,
};
use common::{
    sets::SceneSets,
    structs::{AppConfig, CurrentRealm, DebugInfo, PrimaryUser, SceneImposterBake},
};
use crc::CRC_32_CKSUM;
use ipfs::IpfsAssetServer;
use scene_material::{BoundRegion, SceneBound, SceneMaterial};

use scene_runner::{
    initialize_scene::{
        CurrentImposterScene, LiveScenes, PointerResult, ScenePointers, PARCEL_SIZE,
    },
    renderer_context::RendererSceneContext,
    update_world::gltf_container::{GltfDefinition, GltfProcessed},
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
const TILE_SIZE: u32 = 104;

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
    /// Parcels we've consumed from `unbaked_parcels` so far. Includes empties
    /// — the post-bake write loop iterates this so every parcel gets a
    /// per-parcel spec on disk (empty ones with `imposters: {}`), preventing
    /// the runtime from spinning forever on "spec not found → re-bake →
    /// still empty → re-bake".
    processed_parcels: Vec<IVec2>,
    baked_scene: BakedScene,
}

const SCENE_BAKE_TICK: u32 = 200;
// Additional scene ticks to wait after all textures first complete loading,
// to let scripts / animations / dynamic state settle before capturing.
const SCENE_BAKE_SETTLE_TICKS: u32 = 60;

fn make_scene_oven(
    current_imposter: Res<CurrentImposterScene>,
    mut commands: Commands,
    baking: Query<&ImposterOven>,
    mut scenes: Query<(&mut RendererSceneContext, &mut Transform)>,
    live_scenes: Res<LiveScenes>,
    tick: Res<FrameCount>,
    mut start_tick: Local<Option<(String, u32)>>,
    // (scene hash, scene tick_number at which all textures were first observed loaded)
    mut settle_tick: Local<Option<(String, u32)>>,
    bake_list: Res<ImposterBakeList>,
    children: Query<&Children>,
    mat_handles: Query<&MeshMaterial3d<SceneMaterial>>,
    materials: Res<Assets<SceneMaterial>>,
    asset_server: Res<AssetServer>,
    gltf_state: Query<(Has<GltfDefinition>, Has<GltfProcessed>)>,
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
            if tick.0 > start_tick.as_ref().unwrap().1 + 10000 {
                warn!("scene load failed, spawning dummy oven");
                let Some(ImposterToBake::Scene(parcel, _)) = bake_list.0.last() else {
                    error!("bake list doesn't have a scene, scene entity is missing, but CurrentImposterScene is set?!");
                    return;
                };
                commands.spawn(ImposterOven {
                    start_tick: tick.0,
                    hash: hash.clone(),
                    unbaked_parcels: vec![BoundRegion::new(*parcel, *parcel, 1)],
                    processed_parcels: Vec::new(),
                    baked_scene: BakedScene {
                        crc: crc::Crc::<u32>::new(&CRC_32_CKSUM).checksum(hash.as_bytes()),
                        ..Default::default()
                    },
                });
            }
            return;
        };

        transform.translation.y = -2000.0;

        // While the network is still genuinely resolving an asset for this
        // scene, never arm the `spawning_forever` escape hatch — otherwise a
        // slow CDN response (or a large mesh) over ~30s of frames would
        // cause us to bake without it. "Pending" here means the texture is
        // still in `Loading`/`NotLoaded` or a `GltfDefinition` hasn't been
        // matched by a `GltfProcessed` yet. `Failed` assets *do* let the
        // timeout arm — we don't want a permanently-broken asset to block
        // the bake forever.
        let any_pending_asset = children.iter_descendants(*entity).any(|child| {
            if let Ok((true, false)) = gltf_state.get(child) {
                return true;
            }
            let Ok(h_mat) = mat_handles.get(child) else {
                return false;
            };
            let Some(mat) = materials.get(h_mat.id()) else {
                return false;
            };
            [
                &mat.base.base_color_texture,
                &mat.base.normal_map_texture,
                &mat.base.metallic_roughness_texture,
                &mat.base.emissive_texture,
            ]
            .into_iter()
            .flatten()
            .any(|h| {
                matches!(
                    asset_server.load_state(h.id()),
                    bevy::asset::LoadState::Loading | bevy::asset::LoadState::NotLoaded
                )
            })
        });

        // Reset the timeout baseline on every frame anything is loading, so
        // the 10000-frame countdown only begins once *no* asset is still
        // resolving. Equivalent to "10k frames since the last completion".
        if any_pending_asset {
            *start_tick = Some((hash.clone(), tick.0));
        }
        let spawning_forever = tick.0 > start_tick.as_ref().unwrap().1 + 10000;

        if context.tick_number < SCENE_BAKE_TICK && !context.broken && !spawning_forever {
            return;
        }

        // wait for gltf containers to finish loading (a `GltfDefinition`
        // without a matching `GltfProcessed` is still being instantiated;
        // its meshes/materials aren't in the world yet, so the texture
        // check below would pass vacuously while the scene is incomplete)
        if !spawning_forever {
            for child in children.iter_descendants(*entity) {
                if let Ok((true, false)) = gltf_state.get(child) {
                    return;
                }
            }
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

        // first frame textures are all loaded; record the scene tick and let the
        // scene tick a few more times so dynamic state can settle before capture
        if !spawning_forever
            && settle_tick
                .as_ref()
                .is_none_or(|(settle_hash, _)| hash != settle_hash)
        {
            *settle_tick = Some((hash.clone(), context.tick_number));
        }
        if !spawning_forever {
            let ready_at = settle_tick.as_ref().unwrap().1;
            if context.tick_number < ready_at.saturating_add(SCENE_BAKE_SETTLE_TICKS) {
                return;
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
            processed_parcels: Vec::new(),
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
    current_realm: Res<CurrentRealm>,
    tick: Res<FrameCount>,
    children: Query<&Children>,
    meshes: Query<(&GlobalTransform, &Aabb, &Visibility), With<Mesh3d>>,
    bound_materials: Query<&MeshMaterial3d<SceneMaterial>>,
    mut materials: ResMut<Assets<SceneMaterial>>,
    config: Res<AppConfig>,
    plugin: Res<DclImposterPlugin>,
) {
    let realm = &current_realm.about_url;
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
                // Mip-0 layout: one parcel-keyed spec per parcel under
                // `realms/<realm>/0/`, sharing the scene's CRC. We iterate
                // every parcel we *processed* (not just those that ended up
                // in `baked_scene.imposters`) so empty parcels also get a
                // spec on disk. Without those, an empty parcel's runtime
                // load fails (PendingRemote → 404 → Missing) and
                // `pick_imposter_to_bake` re-queues the same scene every
                // frame — infinite re-bake.
                let scene_crc = oven.baked_scene.crc;
                for parcel in &oven.processed_parcels {
                    let imposter_spec = oven.baked_scene.imposters.get(parcel).copied();
                    let single = BakedScene {
                        imposters: imposter_spec
                            .map(|s| HashMap::from_iter([(*parcel, s)]))
                            .unwrap_or_default(),
                        crc: scene_crc,
                    };
                    write_imposter(ipfas.ipfs_cache_path(), realm, *parcel, 0, &single);
                    if let Some(output_path) = plugin.zip_output.clone() {
                        save_and_zip_callback::<()>(
                            |_| {},
                            spec_path(ipfas.ipfs_cache_path(), realm, *parcel, 0),
                            output_path,
                            realm.clone(),
                            *parcel,
                            0,
                            Some(scene_crc),
                        )(());
                    }
                }

                // delete the scene since we messed with it a lot to get it stable
                commands.entity(current_scene_ent).despawn();
                live_scenes.scenes.remove(&oven.hash);

                current_imposter.0.as_mut().unwrap().1 = true;
                commands.entity(baking_ent).despawn();
                return;
            };

            debug!("baking region: {:?}", region);
            oven.processed_parcels.push(region.parcel_min());

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
                let clamped_min: Vec3 = rmin.max(content_aabb.min().into());
                let clamped_max: Vec3 = rmax.min(content_aabb.max().into());
                // Pull the bottom down to the world floor (y=0) so the
                // display cube rests on the ground. The "skip low things"
                // filter raises `content_aabb.min.y` above 0 for any scene
                // whose lowest meaningful content sits above it; without
                // this the imposter floats. Stay clamped to `rmin.y` for
                // safety. The extra V-range bakes mostly empty and is
                // covered at display by the floor imposter quad.
                let extended_min = Vec3::new(
                    clamped_min.x,
                    clamped_min.y.min(0.0).max(rmin.y),
                    clamped_min.z,
                );
                let aabb = Aabb::from_min_max(extended_min, clamped_max);
                let center = Vec3::from(aabb.center);
                let radius = aabb.half_extents.length();

                // Update materials' SceneBound to allow content past the
                // parcel boundary by the maximum parallax shift this
                // imposter will need at render time. The shift is bounded
                // by `2 × radius × tan(half_inter_tile_angle)` — a function
                // of both the imposter's radius (= bake-camera half-frustum)
                // and the grid resolution. Scaling per-imposter saves bake
                // size on short / flat content (e.g. ground patches at
                // radius ~8m want < 1m of tolerance) while still giving the
                // worst case (radius ~34m for tall cubes) the ~3m it needs.
                // Floored at 1m to match the live-render tolerance baseline
                // and avoid sub-pixel rounding artefacts.
                let half_inter_tile = std::f32::consts::FRAC_PI_2 / GRID_SIZE as f32;
                let bound_tolerance = (2.0 * radius * half_inter_tile.tan()).max(1.0);
                for h_mat in children
                    .iter_descendants(current_scene_ent)
                    .filter_map(|c| bound_materials.get(c).ok())
                {
                    let Some(mat) = materials.get_mut(h_mat) else {
                        continue;
                    };
                    mat.extension.data = SceneBound::new(vec![region], bound_tolerance).data;
                }

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

                let path = texture_path(ipfas.ipfs_cache_path(), realm, region.parcel_min(), 0);
                let _ = std::fs::create_dir_all(path.parent().unwrap());
                let save_asset_callback = make_save_callback(&camera, &path);

                if let Some(output_path) = plugin.zip_output.clone() {
                    camera.set_callback(save_and_zip_callback(
                        save_asset_callback,
                        path,
                        output_path,
                        realm.clone(),
                        region.parcel_min(),
                        0,
                        Some(oven.baked_scene.crc),
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
                        overhang: bound_tolerance,
                    },
                );
            }

            // always generate top-down
            let tile_mid = (rmin + rmax) * 0.5;
            let mut top_down = ImposterBakeCamera {
                grid_size: 1,
                radius: (rmax - rmin).xz().max_element() * 0.5 + 1.0,
                tile_size: ((rmax - rmin).xz().max_element() as u32 + 2) * 4,
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

            let path = floor_path(ipfas.ipfs_cache_path(), realm, region.parcel_min(), 0);
            let save_asset_callback = make_save_callback(&top_down, &path);

            if let Some(output_path) = plugin.zip_output.clone() {
                top_down.set_callback(save_and_zip_callback(
                    save_asset_callback,
                    path,
                    output_path,
                    realm.clone(),
                    region.parcel_min(),
                    0,
                    Some(oven.baked_scene.crc),
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

/// V3-format gating. Returns `Some(threshold)` if the V3 on-disk imposter
/// format should be emitted (the default — V3 is ON). Set `BOIMP_V1=1` to
/// fall back to the legacy callback (the cache won't be readable by current
/// runtimes that expect V3). `BOIMP_V2_THRESHOLD` overrides the default
/// threshold of 10 on the 0-255 RGB-RMSE scale.
fn v2_threshold() -> Option<f32> {
    if std::env::var("BOIMP_V1").is_ok() {
        return None;
    }
    if let Ok(s) = std::env::var("BOIMP_V2_THRESHOLD") {
        return s.parse::<f32>().ok();
    }
    Some(10.0)
}

type SaveCallback = Box<dyn FnOnce(bevy::prelude::Image) + Send + Sync + 'static>;

fn make_save_callback(cam: &ImposterBakeCamera, path: &std::path::Path) -> SaveCallback {
    match v2_threshold() {
        Some(t) => Box::new(cam.save_asset_callback_v2(path, true, t)),
        None => Box::new(cam.save_asset_callback(path, true, true)),
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
        // Same bounds, but with each child's region pushed out by its own
        // `overhang` — used to size the bake camera so it captures the overhang
        // the ingredient cubes expose. The overhang is a world-space amount
        // (the level-0 content lean-over) that does not grow with mip level, so
        // tight / high-level children are not over-inflated.
        let mut emin = Vec3::MAX;
        let mut emax = Vec3::MIN;
        let mut max_child_overhang = 0.0f32;
        let mut child_states: Vec<(IVec2, &'static str)> = Vec::new();
        for offset in [IVec2::ZERO, IVec2::X, IVec2::Y, IVec2::ONE] {
            let key = (*parcel + offset * next_size, level - 1, true);

            let spec_state = manager.get_spec(
                &SceneImposter {
                    parcel: key.0,
                    level: key.1,
                    as_ingredient: false,
                },
                1.0,
            );
            match spec_state {
                crate::render::ImposterSpecState::Ready(SpecStateReady {
                    imposter_data: Some((imposter_spec, _)),
                    ..
                }) => {
                    min = min.min(imposter_spec.region_min);
                    max = max.max(imposter_spec.region_max);
                    max_child_overhang = max_child_overhang.max(imposter_spec.overhang);
                    let push = Vec3::splat(imposter_spec.overhang);
                    emin = emin.min(imposter_spec.region_min - push);
                    emax = emax.max(imposter_spec.region_max + push);
                    child_states.push((key.0, "Ready(data)"));
                }
                crate::render::ImposterSpecState::Pending => panic!(),
                crate::render::ImposterSpecState::Ready(_) => {
                    child_states.push((key.0, "Ready(no-data)"));
                    continue;
                }
                crate::render::ImposterSpecState::Missing => {
                    child_states.push((key.0, "Missing"));
                    continue;
                }
            }
        }

        // skip empty
        if min.cmpgt(max).any() {
            warn!(
                "skipping empty imposterception bake at {parcel:?} level {level}, children: {child_states:?}"
            );
            // oven.baked_scene.imposters.insert(region.parcel_min(), None);
        } else {
            let aabb = Aabb::from_min_max(min, max);
            let center = Vec3::from(aabb.center);
            // Radius reaches the furthest pushed-out corner from the (tight)
            // content centre, so the camera frustum captures the overhang.
            let radius_tight = aabb.half_extents.length();
            let radius = (emax - center).max(center - emin).length();

            // Scale tile resolution by the radius inflation so the block content
            // keeps the pixel density it would have without the overhang
            // headroom — the extra pixels cover the (mostly empty) overhang
            // margin. Otherwise the headroom silently lowers mip resolution.
            let tile_size = ((TILE_SIZE / size) as f32
                * aabb.half_extents.xz().length().max(aabb.half_extents.y)
                / 16.0
                * (radius / radius_tight.max(1e-4)))
            .clamp(2.0, 512.0) as u32;
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
            let save_asset_callback = make_save_callback(&camera, &path);

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
                    overhang: max_child_overhang,
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
                tile_size: (16 + 2) * 4,
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
            let save_asset_callback = make_save_callback(&top_down, &path);

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
            // skip empty regions: nothing to bake and get_spec resolves these
            // to Ready(empty) without a baked file, so picking them would
            // loop forever (the entity never gains children).
            if scene_pointers.crc(imposter.parcel, imposter.level) == Some(0) {
                continue 'imposter;
            }

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
                    let PointerResult::Exists { .. } = pointer_result else {
                        panic!()
                    };
                    // The scene-level bake just wrote per-parcel mip-0 specs
                    // under `realms/<realm>/0/`. Drop any cached resolution
                    // for this parcel's level-0 entry so the next get_spec
                    // re-reads from disk.
                    if let Some(crc) = manager.pointers.crc(*parcel, 0) {
                        manager.clear_mip(*parcel, 0, crc);
                    }
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

                    match manager.get_imposter(
                        &SceneImposter {
                            parcel: key.0,
                            level: key.1,
                            as_ingredient: true,
                        },
                        None,
                    ) {
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
