use bevy::{
    asset::AssetPath,
    diagnostic::FrameCount,
    image::TextureFormatPixelInfo,
    platform::collections::{HashMap, HashSet},
    prelude::*,
    tasks::{IoTaskPool, Task},
};
use common::{structs::DebugInfo, util::TaskExt};
use ipfs::{ipfs_path::IpfsPath, IpfsAssetServer};

#[cfg(target_arch = "wasm32")]
use crate::CHANNELS;
#[cfg(not(target_arch = "wasm32"))]
use tokio::sync::mpsc::unbounded_channel;

use crate::{AssetForProcessing, ProcessingAssetType};

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

pub struct ImageProcessingPlugin;

impl Plugin for ImageProcessingPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<AssetForProcessing>();
        app.add_systems(Update, (check_assets, pipe_events));

        #[cfg(not(target_arch = "wasm32"))]
        {
            let (req_sx, req_rx) = unbounded_channel();
            let (resp_sx, resp_rx) = unbounded_channel();
            app.insert_resource(Channels { req_sx, resp_rx });
            std::thread::spawn(|| {
                bevy::tasks::block_on(crate::processor::process_events(req_rx, resp_sx))
            });
        }

        #[cfg(target_arch = "wasm32")]
        {
            let (req_sx, resp_rx) = CHANNELS.get().unwrap().lock().unwrap().0.take().unwrap();
            app.insert_resource(Channels { req_sx, resp_rx });
        }

        app.init_resource::<ImgReprocessStats>();
    }
}

#[derive(Resource)]
struct Channels {
    req_sx: UnboundedSender<AssetForProcessing>,
    resp_rx: UnboundedReceiver<Result<AssetForProcessing, ()>>,
}

#[derive(Resource, Debug, Default)]
struct ImgReprocessStats {
    alive: bool,
    total: usize,
    skip_imposter: usize,
    skip_unloaded: usize,
    skip_tiny: usize,
    skip_unknown: usize,
    skip_unhashable: usize,
    skip_shouldnt_cache: usize,
    already_compressed: usize,
    started: usize,
    processed: usize,
    failed: usize,
    swapped: usize,
}

fn check_assets(
    mut a: EventReader<AssetEvent<Image>>,
    images: Res<Assets<Image>>,
    mut w: EventWriter<AssetForProcessing>,
    ipfas: IpfsAssetServer,
    mut assets_to_process: Local<Vec<(ProcessingAssetType, IpfsPath, AssetPath)>>,
    mut paths_processed: Local<HashSet<IpfsPath>>,
    mut stats: ResMut<ImgReprocessStats>,
    mut task: Local<Option<Task<(usize, usize, Vec<AssetForProcessing>)>>>,
) {
    #[cfg(not(target_arch = "wasm32"))]
    let Some(cache_root) = ipfas.ipfs_cache_path() else {
        return;
    };

    stats.alive = true;

    for ev in a.read() {
        if let AssetEvent::LoadedWithDependencies { id } = ev {
            stats.total += 1;
            let Some(handle) = ipfas.asset_server().get_id_handle(*id) else {
                // skip non-loaded textures
                stats.skip_unloaded += 1;
                continue;
            };
            let Some(asset_path) = handle.path() else {
                // skip non-loaded textures
                stats.skip_unloaded += 1;
                continue;
            };
            let Some(image) = images.get(*id) else {
                // skip unloaded
                stats.skip_unloaded += 1;
                continue;
            };
            if image.is_compressed() {
                // skip already processed
                stats.already_compressed += 1;
                continue;
            };
            if image.width() as usize
                * image.height() as usize
                * image.texture_descriptor.format.pixel_size()
                <= 1024
                || image.height() <= 2 // bc7 requires 4x4 blocks, we won't save anything
                || image.width() <= 2
            {
                stats.skip_tiny += 1;
                continue;
            }
            if asset_path.to_string().contains("boimp#") {
                // skip gltf assets
                stats.skip_imposter += 1;
                continue;
            }

            let (ty, asset_path) = if asset_path.to_string().contains("#Texture") {
                (
                    ProcessingAssetType::Gltf,
                    asset_path.without_label().clone_owned(),
                )
            } else {
                (ProcessingAssetType::Image, asset_path.clone_owned())
            };
            let Ok(Some(ipfs_path)) = IpfsPath::new_from_path(asset_path.path()) else {
                // skip ... ?
                println!("skip unknown {asset_path:?}");
                stats.skip_unknown += 1;
                continue;
            };

            if paths_processed.contains(&ipfs_path) {
                stats.total -= 1;
                continue;
            }
            paths_processed.insert(ipfs_path.clone());

            if let Ok(None) = ipfs_path.context_free_hash() {
                // skip unhashable
                stats.skip_unhashable += 1;
                continue;
            }

            assets_to_process.push((ty, ipfs_path, asset_path.to_owned()));
        }
    }

    if task.is_none() && !assets_to_process.is_empty() {
        let ipfs = ipfas.ipfs().clone();
        let assets_to_process = std::mem::take(&mut *assets_to_process);
        let cache_root = cache_root.to_owned();
        *task = Some(IoTaskPool::get().spawn(async move {
            let ctx = ipfs.context.read().await;
            let mut skip_unhashable = 0;
            let mut skip_shouldnt_cache = 0;

            let results = assets_to_process
                .into_iter()
                .flat_map(|(ty, ipfs_path, base_path)| {
                    let Some(hash) = ipfs_path.hash(&ctx) else {
                        skip_unhashable += 1;
                        return None;
                    };

                    if ipfs_path.should_cache(&hash) {
                        #[cfg(not(target_arch = "wasm32"))]
                        let cache_path = {
                            let mut cache_path = std::path::PathBuf::from(&cache_root);
                            cache_path.push(hash);
                            cache_path.to_string_lossy().into_owned()
                        };

                        #[cfg(target_arch = "wasm32")]
                        let Ok(cache_path) = ipfs_path.to_url(&ctx) else {
                            skip_unhashable += 1;
                            return None;
                        };

                        Some(AssetForProcessing {
                            cache_path,
                            base_path,
                            ty,
                        })
                    } else {
                        skip_shouldnt_cache += 1;
                        None
                    }
                })
                .collect::<Vec<_>>();

            (skip_unhashable, skip_shouldnt_cache, results)
        }));
    }

    if let Some(mut current_task) = task.take() {
        match current_task.complete() {
            Some((skip_unhashable, skip_shouldnt_cache, results)) => {
                stats.skip_unhashable += skip_unhashable;
                stats.skip_shouldnt_cache += skip_shouldnt_cache;
                for asset in results {
                    w.write(asset);
                }
            }
            None => *task = Some(current_task),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn pipe_events(
    mut channels: ResMut<Channels>,
    mut events: EventReader<AssetForProcessing>,
    asset_server: Res<AssetServer>,
    mut stats: ResMut<ImgReprocessStats>,
    mut debug_info: ResMut<DebugInfo>,
    gltfs: Res<Assets<Gltf>>,
    std_mats: Res<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut processing_gltfs: Local<HashMap<Handle<Gltf>, AssetId<Gltf>>>,
    mut last_good: Local<u32>,
    mut max_bad: Local<u32>,
    tick: Res<FrameCount>,
) {
    for event in events.read() {
        stats.started += 1;
        let _ = channels.req_sx.send(event.clone());
    }

    if stats.started == 0 {
        *last_good = tick.0;
    }

    while let Ok(res) = channels.resp_rx.try_recv() {
        *last_good = tick.0;
        match res {
            Ok(req) => {
                stats.processed += 1;
                stats.started -= 1;

                match req.ty {
                    ProcessingAssetType::Gltf => {
                        if let Some(h) = asset_server.get_handle::<Gltf>(&req.base_path) {
                            processing_gltfs.insert(asset_server.load(req.cache_path), h.id());
                        }
                    }
                    ProcessingAssetType::Image => asset_server.reload(req.base_path),
                };
            }
            Err(_) => {
                stats.started -= 1;
                stats.failed += 1;
            }
        }
    }

    if tick.0 - *last_good > *max_bad {
        *max_bad = tick.0 - *last_good;
        debug!("max bad: {}", *max_bad);
    }

    debug_info.info.insert("processing", format!("{stats:?}"));

    let mut imgs_replaced = 0;
    // gltf reload doesn't work (bevy#18267), so manually overwrite all the images
    processing_gltfs.retain(|h_replace, id_orig| {
        let Some(gltf) = gltfs.get(*id_orig) else {
            return false;
        };

        match asset_server.load_state(h_replace) {
            bevy::asset::LoadState::NotLoaded => return true,
            bevy::asset::LoadState::Loading => return true,
            bevy::asset::LoadState::Failed(_) => return false,
            bevy::asset::LoadState::Loaded => (),
        };

        let Some(replace) = gltfs.get(h_replace) else {
            return true;
        };

        for (h_mat_replace, h_mat) in replace.materials.iter().zip(gltf.materials.iter()) {
            if let (Some(mat_replace), Some(mat)) =
                (std_mats.get(h_mat_replace), std_mats.get(h_mat))
            {
                for (h_replace, h_item) in [
                    (&mat_replace.base_color_texture, &mat.base_color_texture),
                    (&mat_replace.normal_map_texture, &mat.normal_map_texture),
                    (
                        &mat_replace.metallic_roughness_texture,
                        &mat.metallic_roughness_texture,
                    ),
                    (&mat_replace.emissive_texture, &mat.emissive_texture),
                ] {
                    if let (Some(h_replace_img), Some(h_img)) = (h_replace.clone(), h_item.clone())
                    {
                        let Some(img) = images.get(&h_replace_img) else {
                            continue;
                        };

                        let img = img.clone();
                        images.insert(h_img.id(), img.clone());
                        imgs_replaced += 1;
                    }
                }
            }
        }

        false
    });

    if imgs_replaced > 0 {
        debug!("replaced {} imgs", imgs_replaced);
        stats.swapped += imgs_replaced;
    }
}
