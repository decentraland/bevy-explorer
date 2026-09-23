//! Packed, original micro-relief/fleck texture. Generated once, not per pixel
//! every frame. RG = slopes, B = moss flecks, A = diffuse value variation.
use crate::tree_geometry::random;
use bevy::{
    asset::{weak_handle, RenderAssetUsages},
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

pub(super) const MEADOW: Handle<Image> = weak_handle!("b84e6da6-2a2d-465f-82da-f3ac1b34462c");
const SIZE: usize = 256;

pub(super) fn image() -> Image {
    let data: Vec<_> = (0..SIZE * SIZE)
        .map(|i| {
            let x = i % SIZE;
            let y = i / SIZE;
            let seed = ((x / 4) as u32 * 733) ^ ((y / 8) as u32 * 179);
            let delta = Vec2::new((x % 4) as f32 - 1.5, (y % 8) as f32 - 3.5);
            let angle = (random(seed ^ 113) - 0.5) * 0.7;
            let axis = Vec2::from_angle(angle);
            let p = Vec2::new(delta.dot(axis) / 1.4, delta.perp_dot(axis) / 3.8);
            let leaf = (1.0 - p.length_squared()).max(0.0) * f32::from(random(seed) > 0.25);
            let grain = random(i as u32 * 23 + 71);
            let height = leaf * 0.004 + grain * 0.0015;
            let moss = leaf * f32::from(random(seed ^ 419) > 0.67);
            let value = 0.78 + grain * 0.16 + leaf * 0.10;
            (height, moss, value)
        })
        .collect();
    let height = |x: usize, y: usize| data[(y % SIZE) * SIZE + x % SIZE].0;
    let mut level: Vec<[u8; 4]> = (0..SIZE * SIZE)
        .map(|i| {
            let (x, y) = (i % SIZE, i / SIZE);
            let dx = (height(x + 1, y) - height(x + SIZE - 1, y)) * 16.0;
            let dz = (height(x, y + 1) - height(x, y + SIZE - 1)) * 16.0;
            [
                ((0.5 - dx) * 255.0) as u8,
                ((0.5 - dz) * 255.0) as u8,
                (data[i].1 * 255.0) as u8,
                (data[i].2 * 255.0) as u8,
            ]
        })
        .collect();
    let mut bytes = Vec::new();
    let mut size = SIZE;
    loop {
        bytes.extend(level.iter().flatten().copied());
        if size == 1 {
            break;
        }
        let half = size / 2;
        level = (0..half * half)
            .map(|i| {
                let origin = i / half * 2 * size + i % half * 2;
                std::array::from_fn(|channel| {
                    [origin, origin + 1, origin + size, origin + size + 1]
                        .into_iter()
                        .map(|p| u32::from(level[p][channel]))
                        .sum::<u32>()
                        .div_ceil(4) as u8
                })
            })
            .collect();
        size = half;
    }
    let mut image = Image::new_fill(
        Extent3d {
            width: SIZE as u32,
            height: SIZE as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0; 4],
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.mip_level_count = 9;
    image.data = Some(bytes);
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 8,
        ..default()
    });
    image
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packed_turf_is_deterministic_mipmapped_and_has_neutral_distant_normals() {
        let image = image();
        let bytes = image.data.unwrap();
        assert_eq!(bytes.len(), 349524);
        assert_eq!(image.texture_descriptor.mip_level_count, 9);
        assert_eq!(bytes, super::image().data.unwrap());
        let last = &bytes[bytes.len() - 4..];
        assert!((125..=132).contains(&last[0]));
        assert!((125..=132).contains(&last[1]));
        assert!(bytes.chunks_exact(4).any(|p| p[2] > 150));
    }
}
