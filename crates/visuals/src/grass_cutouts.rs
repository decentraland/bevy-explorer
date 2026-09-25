//! Original leaf silhouettes, baked once instead of evaluated per fragment.
use bevy::{
    asset::RenderAssetUsages,
    image::{ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::{
        Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
    },
};

const SIZE: usize = 128;
const VARIANTS: usize = 8;

fn inside(x: f32, y: f32, seed: u32, shape: Vec4) -> bool {
    let x = x * shape.x;
    let lane = x.floor();
    (-2..=2).any(|offset| {
        let leaf = lane + offset as f32;
        let r = crate::grass_blades::random(seed ^ (leaf as i32 as u32).wrapping_mul(7937));
        let t = y / (0.98 - shape.w + shape.w * r);
        let center = leaf + 0.5 + (r - 0.5) * 0.35 + (r * 1.6 - 0.55) * t * t * shape.z;
        // Narrow roots, a soft shoulder and a long tapered, curved tip.
        let width = shape.y * (0.45 + 1.8 * t) * (1.0 - t).max(0.0).powf(0.8);
        (0.0..shape.x).contains(&leaf) && t <= 1.0 && (x - center).abs() < width
    })
}

pub(super) fn image(shape: Vec4) -> Image {
    let mut bytes = Vec::new();
    // Layer-major, matching Image's default TextureDataOrder.
    for variant in 0..VARIANTS {
        let mut level: Vec<u8> = (0..SIZE * SIZE)
            .map(|pixel| {
                let (x, y) = (pixel % SIZE, pixel / SIZE);
                let mut hits = 0;
                for dy in [0.25, 0.75] {
                    for dx in [0.25, 0.75] {
                        hits += u32::from(inside(
                            (x as f32 + dx) / SIZE as f32,
                            (y as f32 + dy) / SIZE as f32,
                            variant as u32 * 179 + 17,
                            shape,
                        ));
                    }
                }
                (hits * 255 / 4) as u8
            })
            .collect();
        let target_coverage =
            level.iter().filter(|&&a| a >= 128).count() as f32 / level.len() as f32;
        let mut size = SIZE;
        loop {
            bytes.extend_from_slice(&level);
            if size == 1 {
                break;
            }
            let next_size = size / 2;
            let mut next: Vec<u8> = (0..next_size * next_size)
                .map(|pixel| {
                    let origin = (pixel / next_size) * 2 * size + (pixel % next_size) * 2;
                    [origin, origin + 1, origin + size, origin + size + 1]
                        .into_iter()
                        .map(|i| level[i] as u32)
                        .sum::<u32>()
                        .div_ceil(4) as u8
                })
                .collect();
            // Preserve alpha-test coverage as leaves become subpixel. Ordinary
            // averaging otherwise erases thin foliage early and causes shimmer.
            let (mut lo, mut hi) = (0.25, 4.0);
            for _ in 0..10 {
                let scale = (lo + hi) * 0.5;
                let coverage = next.iter().filter(|&&a| a as f32 * scale >= 128.0).count() as f32
                    / next.len() as f32;
                if coverage < target_coverage {
                    lo = scale;
                } else {
                    hi = scale;
                }
            }
            for alpha in &mut next {
                *alpha = (*alpha as f32 * hi).min(255.0) as u8;
            }
            level = next;
            size = next_size;
        }
    }
    let mut image = Image::new_fill(
        Extent3d {
            width: SIZE as u32,
            height: SIZE as u32,
            depth_or_array_layers: VARIANTS as u32,
        },
        TextureDimension::D2,
        &[0],
        TextureFormat::R8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.mip_level_count = SIZE.ilog2() + 1;
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::D2Array),
        ..default()
    });
    image.data = Some(bytes);
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 4,
        ..default()
    });
    image
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaf_settings_change_the_masks_at_the_same_texture_budget() {
        let mut masks = Vec::new();
        for leaves in [
            crate::grass_look::GrassLook::default().leaves,
            Vec4::new(10.0, 0.16, 0.65, 0.30),
            Vec4::new(6.0, 0.31, 0.85, 0.22),
        ] {
            let bytes = image(leaves).data.unwrap();
            assert_eq!(bytes.len(), 174_760);
            assert!(bytes[..SIZE * SIZE].iter().any(|&a| a >= 128));
            assert!(masks.iter().all(|other| *other != bytes));
            masks.push(bytes);
        }
    }

    #[test]
    fn masks_are_small_mipmapped_varied_and_have_leaf_coverage() {
        let image = image(crate::grass_look::GrassLook::default().leaves);
        let bytes = image.data.unwrap();
        let stride: usize = (0..8).map(|mip| (SIZE >> mip).pow(2)).sum();
        assert_eq!(bytes.len(), stride * VARIANTS);
        assert!(bytes.len() < 180_000);
        assert_eq!(image.texture_descriptor.mip_level_count, 8);
        for variant in 0..VARIANTS {
            let base = &bytes[variant * stride..variant * stride + SIZE * SIZE];
            let coverage = base.iter().filter(|&&a| a >= 128).count();
            assert!((SIZE * SIZE / 8..SIZE * SIZE / 2).contains(&coverage));
            assert!(base[(SIZE - 1) * SIZE..].iter().all(|&a| a == 0));
            if variant > 0 {
                assert_ne!(base, &bytes[..SIZE * SIZE]);
            }
        }
    }
}
