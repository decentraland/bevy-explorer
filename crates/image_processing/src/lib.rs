use std::{collections::VecDeque, io::Write, path::PathBuf};

use bevy_console::ConsoleCommand;
use block_compression::BC7Settings;
use common::structs::DebugInfo;
use console::DoAddConsoleCommand;
use ddsfile::{Caps2, Dds, DxgiFormat};
use gltf::Glb;
use ipfs::{ipfs_path::IpfsPath, IpfsAssetServer};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use bevy::{
    asset::AssetPath,
    image::TextureFormatPixelInfo,
    platform::collections::{HashMap, HashSet},
    prelude::*,
};

#[derive(Clone, Copy)]
pub enum ProcessingAssetType {
    Gltf,
    Image,
}

#[derive(Event, Clone)]
pub struct AssetForProcessing {
    pub cache_path: PathBuf,
    pub base_path: AssetPath<'static>,
    pub ty: ProcessingAssetType,
}

pub struct ImageProcessingPlugin;

impl Plugin for ImageProcessingPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<AssetForProcessing>();
        app.add_systems(Update, (check_assets, pipe_events));

        let (req_sx, req_rx) = unbounded_channel();
        let (resp_sx, resp_rx) = unbounded_channel();

        app.insert_resource(Channels { req_sx, resp_rx });
        app.init_resource::<ImgReprocessStats>();

        std::thread::spawn(|| process_events(req_rx, resp_sx));

        app.add_console_command::<ForceReloadGltfs, _>(force_reload_gltfs);
    }
}

#[derive(Resource)]
struct Channels {
    req_sx: UnboundedSender<AssetForProcessing>,
    resp_rx: UnboundedReceiver<Result<AssetForProcessing, ()>>,
}

#[derive(Resource, Debug, Default)]
struct ImgReprocessStats {
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
    mut assets_to_process: Local<VecDeque<(ProcessingAssetType, IpfsPath, AssetPath)>>,
    mut paths_processed: Local<HashSet<IpfsPath>>,
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
            if image.width() as usize
                * image.height() as usize
                * image.texture_descriptor.format.pixel_size()
                < 1024
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

            assets_to_process.push_back((ty, ipfs_path, asset_path.to_owned()));
        }
    }

    if assets_to_process.is_empty() {
        return;
    }

    let Ok(ctx) = ipfas.ipfs().context.try_read() else {
        return;
    };

    for (ty, ipfs_path, base_path) in assets_to_process.drain(..) {
        let Some(hash) = ipfs_path.hash(&ctx) else {
            stats.skip_unhashable += 1;
            continue;
        };

        if ipfs_path.should_cache(&hash) {
            let mut cache_path = PathBuf::from(cache_root);
            cache_path.push(hash);

            w.write(AssetForProcessing {
                cache_path,
                base_path,
                ty,
            });
        } else {
            stats.skip_shouldnt_cache += 1;
        }
    }
}

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
) {
    for event in events.read() {
        stats.started += 1;
        let _ = channels.req_sx.send(event.clone());
    }

    while let Ok(res) = channels.resp_rx.try_recv() {
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

    debug_info.info.insert("processing", format!("{:?}", stats));

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
        error!("replaced {} imgs", imgs_replaced);
        stats.swapped += imgs_replaced;
    }
}

fn process_events(
    mut req_rx: UnboundedReceiver<AssetForProcessing>,
    resp_sx: UnboundedSender<Result<AssetForProcessing, ()>>,
) {
    while let Some(req) = req_rx.blocking_recv() {
        let Ok(raw_bytes) = std::fs::read(&req.cache_path) else {
            println!("can't read {:?}", req.cache_path);
            let _ = resp_sx.send(Err(()));
            continue;
        };

        let data = match req.ty {
            ProcessingAssetType::Gltf => match process_gltf(&raw_bytes) {
                Ok(gltf) => gltf,
                Err(e) => {
                    println!("failed to process image {:?}: {:?}", req.base_path, e);
                    let _ = resp_sx.send(Err(()));
                    continue;
                }
            },
            ProcessingAssetType::Image => match process_image(&raw_bytes) {
                Ok(dds) => dds,
                Err(e) => {
                    println!("failed to process gltf {:?}: {:?}", req.base_path, e);
                    let _ = resp_sx.send(Err(()));
                    continue;
                }
            },
        };

        let Ok(mut file_output) = std::fs::File::create(&req.cache_path) else {
            println!("can't write {:?}", req.base_path);
            let _ = resp_sx.send(Err(()));
            continue;
        };

        let Ok(()) = file_output.write_all(&data) else {
            println!("can't write {:?}", req.base_path);
            let _ = resp_sx.send(Err(()));
            continue;
        };
        let _ = resp_sx.send(Ok(req));
    }
}

#[derive(Debug)]
enum ImageProcessError {
    CantLoad,
    DdsFailed,
}

fn process_image(raw_bytes: &[u8]) -> Result<Vec<u8>, ImageProcessError> {
    let Ok(img) = image::load_from_memory(&raw_bytes) else {
        return Err(ImageProcessError::CantLoad);
    };

    let initial_width = img.width();
    let initial_height = img.height();
    let resized = if initial_width > 1024
        || initial_height > 1024
        || !initial_width.is_multiple_of(4)
        || !initial_height.is_multiple_of(4)
    {
        let downratio = (1024.0 / initial_width.max(initial_height) as f32).min(1.0);
        let resized_width = (initial_width as f32 * downratio * 0.25).round() as u32 * 4;
        let resized_height = (initial_height as f32 * downratio * 0.25).round() as u32 * 4;
        img.resize_exact(
            resized_width,
            resized_height,
            image::imageops::FilterType::CatmullRom,
        )
    } else {
        img
    };

    let width = resized.width();
    let height = resized.height();

    let rgba_pixels = resized.to_rgba8().into_raw();

    let mut compressed_data = vec![0u8; (width * height) as usize];

    println!(
        "size: {}/{} -> {}/{}",
        initial_width,
        initial_height,
        resized.width(),
        resized.height()
    );
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
        return Err(ImageProcessError::DdsFailed);
    };

    dds.data = compressed_data;

    let mut output = Vec::new();
    let Ok(()) = dds.write(&mut output) else {
        return Err(ImageProcessError::DdsFailed);
    };

    Ok(output)
}

#[derive(Debug)]
enum GltfProcessError {
    #[allow(dead_code)] // we use the ignored debug impl
    ImageError(ImageProcessError),
    InvalidHeader,
}

impl From<ImageProcessError> for GltfProcessError {
    fn from(value: ImageProcessError) -> Self {
        Self::ImageError(value)
    }
}

fn process_gltf(raw_bytes: &[u8]) -> Result<Vec<u8>, GltfProcessError> {
    let (mut root, old_bin): (gltf_json::Root, Vec<u8>) = if raw_bytes.starts_with(b"glTF") {
        let glb = Glb::from_slice(raw_bytes).unwrap();
        (
            gltf_json::deserialize::from_slice(&glb.json).unwrap(),
            glb.bin.map(|b| b.into_owned()).unwrap_or_default(),
        )
    } else if raw_bytes.starts_with(b"{") {
        (
            gltf_json::deserialize::from_slice(raw_bytes).unwrap(),
            Vec::default(),
        )
    } else {
        return Err(GltfProcessError::InvalidHeader);
    };

    // A. Collect all Raw Image Data first
    // We store them in a temp vector so we can safely mutate the JSON later.
    let mut raw_images: Vec<Option<Vec<u8>>> = Vec::new();
    let mut zombie_views = vec![false; root.buffer_views.len()];

    for image in &root.images {
        let pixels = extract_pixels_safe(image, &root.buffer_views, &old_bin);
        raw_images.push(pixels);

        // Mark associated view as zombie
        if let Some(idx) = image.buffer_view {
            zombie_views[idx.value()] = true;
        }
    }

    // B. Rebuild Binary (Geometry Only)
    let mut new_bin: Vec<u8> = Vec::new();

    for (i, view) in root.buffer_views.iter_mut().enumerate() {
        if zombie_views[i] {
            // Nullify zombie views
            view.byte_length = 0u64.into();
            view.byte_offset = Some(0u64.into());
        } else {
            // Copy geometry data
            let start = view.byte_offset.unwrap_or_default().0 as usize;
            let len = view.byte_length.0 as usize;
            // Pad
            while new_bin.len() % 4 != 0 {
                new_bin.push(0);
            }
            let new_offset = new_bin.len() as u64;

            new_bin.extend_from_slice(&old_bin[start..start + len]);
            view.byte_offset = Some(new_offset.into());
        }
    }

    // C. Compress & Append Textures
    for (i, raw_opt) in raw_images.into_iter().enumerate() {
        if let Some(raw_pixels) = raw_opt {
            // 1. Resize & Compress (Your existing BC7 logic)
            // Note: This needs to handle the "Pad to 4x4" logic we discussed
            let bc7_data = process_image(&raw_pixels)?;

            // 2. Append to new_bin
            while new_bin.len() % 4 != 0 {
                new_bin.push(0);
            }
            let offset = new_bin.len() as u64;
            new_bin.extend_from_slice(&bc7_data);

            // 3. Create NEW BufferView
            let new_view_idx = root.buffer_views.len();
            root.buffer_views.push(gltf_json::buffer::View {
                buffer: gltf_json::Index::new(0),
                byte_length: bc7_data.len().into(),
                byte_offset: Some(offset.into()),
                byte_stride: None,
                name: Some("BC7_Data".into()),
                target: None,
                extensions: None,
                extras: Default::default(),
            });

            // 4. Update Image to point to new View
            root.images[i].buffer_view = Some(gltf_json::Index::new(new_view_idx as u32));
            root.images[i].uri = None; // Ensure URI is gone
            root.images[i].mime_type = Some(gltf_json::image::MimeType("image/vnd-ms.dds".into()));
        }
    }

    // D. Update Buffer Total Size
    if root.buffers.is_empty() {
        root.buffers.push(gltf_json::Buffer {
            byte_length: Default::default(),
            name: None,
            uri: None,
            extensions: None,
            extras: None,
        });
    }
    root.buffers[0].byte_length = new_bin.len().into();

    // E. Serialize (using previous helper)
    let mut output = Vec::new();
    write_glb(&mut output, &root, &new_bin).unwrap();
    Ok(output)
}

// Helper that doesn't borrow 'root' mutably
fn extract_pixels_safe(
    img: &gltf_json::Image,
    views: &[gltf_json::buffer::View],
    bin: &[u8],
) -> Option<Vec<u8>> {
    use base64::{engine::general_purpose, Engine as _};

    if let Some(uri) = &img.uri {
        if uri.starts_with("data:") {
            let parts: Vec<&str> = uri.split(',').collect();
            return general_purpose::STANDARD.decode(parts[1]).ok();
        }
    }
    if let Some(idx) = img.buffer_view {
        let view = &views[idx.value()];
        let start = view.byte_offset.unwrap_or_default().0 as usize;
        let end = start + view.byte_length.0 as usize;
        if end <= bin.len() {
            return Some(bin[start..end].to_vec());
        }
    }
    None
}

pub fn write_glb<W: Write>(
    writer: &mut W,
    root: &gltf_json::Root,
    binary_payload: &[u8],
) -> std::io::Result<()> {
    // 1. Serialize JSON
    let json_string = gltf_json::serialize::to_string(root).expect("Serialization failed");
    let mut json_bytes = json_string.as_bytes().to_vec();

    // 2. Pad JSON (Must be multiple of 4, padded with spaces 0x20)
    while json_bytes.len() % 4 != 0 {
        json_bytes.push(0x20);
    }

    // 3. Pad Binary (Must be multiple of 4, padded with zeros 0x00)
    // Note: We copy to a vector here to pad. For huge files,
    // you might want to write padding bytes directly to the writer instead.
    let mut bin_data = binary_payload.to_vec();
    while bin_data.len() % 4 != 0 {
        bin_data.push(0x00);
    }

    // 4. Calculate Header
    let json_len = json_bytes.len() as u32;
    let bin_len = bin_data.len() as u32;
    let total_len = 12                   // Header
                  + 8 + json_len         // JSON Chunk (Length + Type + Data)
                  + 8 + bin_len; // BIN Chunk (Length + Type + Data)

    // 5. Write Header
    writer.write_all(b"glTF")?; // Magic
    writer.write_all(&2u32.to_le_bytes())?; // Version
    writer.write_all(&total_len.to_le_bytes())?; // Total Length

    // 6. Write JSON Chunk
    writer.write_all(&json_len.to_le_bytes())?;
    writer.write_all(b"JSON")?;
    writer.write_all(&json_bytes)?;

    // 7. Write Binary Chunk
    writer.write_all(&bin_len.to_le_bytes())?;
    writer.write_all(b"BIN\0")?;
    writer.write_all(&bin_data)?;

    Ok(())
}

// set thread count
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/frg")]
struct ForceReloadGltfs;

fn force_reload_gltfs(
    mut input: ConsoleCommand<ForceReloadGltfs>,
    gltfs: Res<Assets<Gltf>>,
    server: Res<AssetServer>,
) {
    if let Some(Ok(_)) = input.take() {
        for (id, _) in gltfs.iter() {
            if let Some(h) = server.get_id_handle(id) {
                if let Some(path) = h.path() {
                    server.reload(path)
                }
            }
        }
    }
}
