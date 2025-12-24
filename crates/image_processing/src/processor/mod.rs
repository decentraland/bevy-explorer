#[cfg(not(target_arch = "wasm32"))]
mod native_fs;
#[cfg(target_arch = "wasm32")]
mod wasm_fs;
use std::io::Write;

use base64::{engine::general_purpose, Engine};
use bevy::log::tracing::{error, info};
use block_compression::BC7Settings;
use ddsfile::{Caps2, Dds, DxgiFormat};
use gltf::Glb;
#[cfg(not(target_arch = "wasm32"))]
use native_fs::{read_file, write_file};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
#[cfg(target_arch = "wasm32")]
use wasm_fs::{read_file, write_file};

use crate::{AssetForProcessing, ProcessingAssetType};

pub(crate) async fn process_events(
    mut req_rx: UnboundedReceiver<AssetForProcessing>,
    resp_sx: UnboundedSender<Result<AssetForProcessing, ()>>,
) {
    while let Some(req) = req_rx.blocking_recv() {
        let Ok(raw_bytes) = read_file(&req.cache_path).await else {
            error!("can't read {:?}", req.cache_path);
            let _ = resp_sx.send(Err(()));
            continue;
        };

        let data = match req.ty {
            ProcessingAssetType::Gltf => match process_gltf(&raw_bytes) {
                Ok(gltf) => gltf,
                Err(e) => {
                    error!("failed to process image {:?}: {:?}", req.base_path, e);
                    let _ = resp_sx.send(Err(()));
                    continue;
                }
            },
            ProcessingAssetType::Image => match process_image(&raw_bytes) {
                Ok(dds) => dds,
                Err(e) => {
                    error!("failed to process gltf {:?}: {:?}", req.base_path, e);
                    let _ = resp_sx.send(Err(()));
                    continue;
                }
            },
        };

        let Ok(()) = write_file(&req.cache_path, &data).await else {
            error!("can't write {:?}", req.base_path);
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
    let Ok(img) = image::load_from_memory(raw_bytes) else {
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

    info!(
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

    if dds.get_height() == 0 || dds.get_width() == 0 {
        error!(
            "returned size zero processing {}/{} -> {}/{}",
            initial_width,
            initial_height,
            resized.width(),
            resized.height()
        );
        return Err(ImageProcessError::DdsFailed);
    }

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
    let (mut root, mut old_bin): (gltf_json::Root, Vec<u8>) = if raw_bytes.starts_with(b"glTF") {
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

    // ai code below
    // absorb data URI buffers into the binary chunk
    if let Some(first_buffer) = root.buffers.first_mut() {
        if let Some(uri) = &first_buffer.uri {
            if uri.starts_with("data:") {
                // This is a .gltf with embedded binary.
                // We should decode it and make it the "real" binary chunk.
                let parts: Vec<&str> = uri.split(',').collect();
                let decoded = general_purpose::STANDARD.decode(parts[1]).unwrap();

                // If we already had a binary chunk (from a GLB), append this.
                // If we came from JSON, 'old_bin' was empty, so this becomes the start.
                // (But usually, valid GLTF/GLB is mutually exclusive on this).
                if old_bin.is_empty() {
                    // This is our new base binary!
                    // We treat this decoded data as the "old_bin" for the rest of the logic.
                    old_bin = decoded;
                }

                // Clear the URI so it becomes a valid GLB buffer
                first_buffer.uri = None;
            }
        }
    }

    // collect all Raw Image Data first
    // store them in a temp vector so we can safely mutate the JSON later.
    let mut raw_images: Vec<Option<Vec<u8>>> = Vec::new();
    let mut zombie_views = vec![false; root.buffer_views.len()];

    for image in &root.images {
        let pixels = extract_pixels_safe(image, &root.buffer_views, &old_bin);
        raw_images.push(pixels);

        // mark associated view as zombie
        if let Some(idx) = image.buffer_view {
            zombie_views[idx.value()] = true;
        }
    }

    // rebuild binary (non-image chunks only)
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
            while !new_bin.len().is_multiple_of(4) {
                new_bin.push(0);
            }
            let new_offset = new_bin.len() as u64;

            new_bin.extend_from_slice(&old_bin[start..start + len]);
            view.byte_offset = Some(new_offset.into());
        }
    }

    // compress & append textures
    for (i, raw_opt) in raw_images.into_iter().enumerate() {
        if let Some(raw_pixels) = raw_opt {
            let bc7_data = process_image(&raw_pixels)?;

            // append to new_bin
            while !new_bin.len().is_multiple_of(4) {
                new_bin.push(0);
            }
            let offset = new_bin.len() as u64;
            new_bin.extend_from_slice(&bc7_data);

            // Create NEW BufferView
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

            // Update Image to point to new View
            root.images[i].buffer_view = Some(gltf_json::Index::new(new_view_idx as u32));
            root.images[i].uri = None;
            root.images[i].mime_type = Some(gltf_json::image::MimeType("image/vnd-ms.dds".into()));
        }
    }

    // Update Buffer Total Size
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

    // Serialize
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
    // ai below
    // 1. Serialize JSON
    let json_string = gltf_json::serialize::to_string(root).expect("Serialization failed");
    let mut json_bytes = json_string.as_bytes().to_vec();

    // 2. Pad JSON (Must be multiple of 4, padded with spaces 0x20)
    while !json_bytes.len().is_multiple_of(4) {
        json_bytes.push(0x20);
    }

    // 3. Pad Binary (Must be multiple of 4, padded with zeros 0x00)
    // Note: We copy to a vector here to pad. For huge files,
    // you might want to write padding bytes directly to the writer instead.
    let mut bin_data = binary_payload.to_vec();
    while !bin_data.len().is_multiple_of(4) {
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
