use std::collections::VecDeque;

use async_std::path::PathBuf;
use block_compression::BC7Settings;
use common::structs::DebugInfo;
use ddsfile::{Caps2, Dds, DxgiFormat};
use ipfs::{ipfs_path::IpfsPath, IpfsAssetServer};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use bevy::{asset::AssetPath, image::TextureFormatPixelInfo, prelude::*};

#[derive(Event, Clone)]
pub struct ImageForProcessing {
    pub cache_path: PathBuf,
    pub base_path: AssetPath<'static>,
}

pub struct ImageProcessingPlugin;

impl Plugin for ImageProcessingPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<ImageForProcessing>();
        app.add_systems(Update, (check_assets, pipe_events));

        let (req_sx, req_rx) = unbounded_channel();
        let (resp_sx, resp_rx) = unbounded_channel();

        app.insert_resource(Channels { req_sx, resp_rx });
        app.init_resource::<ImgReprocessStats>();

        std::thread::spawn(|| process_events(req_rx, resp_sx));
    }
}

#[derive(Resource)]
struct Channels {
    req_sx: UnboundedSender<ImageForProcessing>,
    resp_rx: UnboundedReceiver<Result<ImageForProcessing, ()>>,
}

#[derive(Resource, Debug, Default)]
struct ImgReprocessStats {
    total: usize,
    skip_imposter: usize,
    skip_unloaded: usize,
    skip_gltf: usize,
    skip_tiny: usize,
    skip_unknown: usize,
    skip_unhashable: usize,
    skip_shouldnt_cache: usize,
    already_compressed: usize,
    started: usize,
    processed: usize,
    failed: usize,
}

fn check_assets(
    mut a: EventReader<AssetEvent<Image>>,
    images: Res<Assets<Image>>,
    mut w: EventWriter<ImageForProcessing>,
    ipfas: IpfsAssetServer,
    mut to_process: Local<VecDeque<(IpfsPath, AssetPath)>>,
    mut stats: ResMut<ImgReprocessStats>,
) {
    let Some(cache_root) = ipfas.ipfs_cache_path() else {
        return;
    };

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
            if image.is_compressed() || image.texture_descriptor.format.pixel_size() > 32 {
                // skip already processed
                stats.already_compressed += 1;
                continue;
            };
            if image.width() as usize * image.height() as usize * image.texture_descriptor.format.pixel_size() < 1024 {
                stats.skip_tiny += 1;
                continue;
            }
            if asset_path.to_string().contains("#Texture") {
                // skip gltf assets
                stats.skip_gltf += 1;
                continue;
            }
            if asset_path.to_string().contains("boimp#") {
                // skip gltf assets
                stats.skip_imposter += 1;
                continue;
            }            
            let Ok(Some(ipfs_path)) = IpfsPath::new_from_path(asset_path.path()) else {
                // skip ... ?
                println!("skip unknown {asset_path:?}");
                stats.skip_unknown += 1;
                continue;
            };
            if let Ok(None) = ipfs_path.context_free_hash() {
                // skip unhashable
                stats.skip_unhashable += 1;
                continue;
            }

            to_process.push_back((ipfs_path, asset_path.to_owned()));
        }
    }

    if to_process.is_empty() {
        return;
    }

    let Ok(ctx) = ipfas.ipfs().context.try_read() else {
        return;
    };

    for (ipfs_path, base_path) in to_process.drain(..) {
        let Some(hash) = ipfs_path.hash(&ctx) else {
            stats.skip_unhashable += 1;
            continue;
        };

        if ipfs_path.should_cache(&hash) {
            let mut cache_path = PathBuf::from(cache_root);
            cache_path.push(hash);

            w.write(ImageForProcessing {
                cache_path,
                base_path,
            });
        } else {
            stats.skip_shouldnt_cache += 1;
        }
    }
}

fn pipe_events(
    mut channels: ResMut<Channels>,
    mut events: EventReader<ImageForProcessing>,
    asset_server: Res<AssetServer>,
    mut stats: ResMut<ImgReprocessStats>,
    mut debug_info: ResMut<DebugInfo>,
) {
    for event in events.read() {
        stats.started += 1;
        let _ = channels.req_sx.send(event.clone());
    }

    while let Ok(res) = channels.resp_rx.try_recv() {
        match res {
            Ok(img) => {
                stats.processed += 1;
                stats.started -= 1;
                asset_server.reload(img.base_path);                
            },
            Err(_) => {
                stats.started -= 1;
                stats.failed += 1;
            },
        }
    }

    debug_info.info.insert("processing", format!("{:?}", stats));
}

fn process_events(
    mut req_rx: UnboundedReceiver<ImageForProcessing>,
    resp_sx: UnboundedSender<Result<ImageForProcessing, ()>>,
) {
    while let Some(img_req) = req_rx.blocking_recv() {
        let Ok(raw_bytes) = std::fs::read(&img_req.cache_path) else {
            println!("can't read {:?}", img_req.cache_path);
            let _ = resp_sx.send(Err(()));
            continue;
        };

        let Ok(img) = image::load_from_memory(&raw_bytes) else {
            println!("can't load {:?}", img_req.base_path);
            println!("{:?} contains? {}", img_req.base_path.path().to_string_lossy(), img_req.base_path.path().to_string_lossy().contains("#Texture"));
            let _ = resp_sx.send(Err(()));
            continue;
        };

        let initial_width = img.width();
        let initial_height = img.height();
        let resized = if initial_width > 1024 || initial_height > 1024 || !initial_width.is_multiple_of(4) || !initial_height.is_multiple_of(4) {
            let downratio = (1024.0 / initial_width.max(initial_height) as f32).min(1.0);
            let resized_width = (initial_width as f32 * downratio * 0.25).round() as u32 * 4;
            let resized_height = (initial_height as f32 * downratio * 0.25).round() as u32 * 4;
            img.resize_exact(resized_width, resized_height, image::imageops::FilterType::CatmullRom)
        } else {
            img
        };

        let width = resized.width();
        let height = resized.height();

        let rgba_pixels = resized.to_rgba8().into_raw();

        let mut compressed_data = vec![0u8; (width * height) as usize];

        println!("size: {}/{} -> {}/{}", initial_width, initial_height, resized.width(), resized.height());
        block_compression::encode::compress_rgba8(
            block_compression::CompressionVariant::BC7(BC7Settings::alpha_ultrafast()),
            &rgba_pixels,
            &mut compressed_data,
            width,
            height,
            width * 4,
        );

        let Ok(mut dds) = Dds::new_dxgi(ddsfile::NewDxgiParams {
            height,
            width,
            depth: None,
            format: DxgiFormat::BC7_UNorm,
            mipmap_levels: None,
            array_layers: None,
            caps2: Some(Caps2::empty()),
            is_cubemap: false,
            resource_dimension: ddsfile::D3D10ResourceDimension::Texture2D,
            alpha_mode: ddsfile::AlphaMode::PreMultiplied,
        }) else {
            println!("failed dds header");
            let _ = resp_sx.send(Err(()));
            continue;
        };

        dds.data = compressed_data;

        let Ok(mut file_output) = std::fs::File::create(&img_req.cache_path) else {
            println!("can't write {:?}", img_req.base_path);
            let _ = resp_sx.send(Err(()));
            continue;
        };

        dds.write(&mut file_output).expect("Failed to write DDS");
        let _ = resp_sx.send(Ok(img_req));
    }
}
