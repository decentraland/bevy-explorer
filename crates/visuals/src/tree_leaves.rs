//! Original oval leaf sprays, generated once. No reference artwork is sampled.
use bevy::{
    asset::RenderAssetUsages,
    image::{ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

use crate::tree_geometry::random;

const SIZE: usize = 256;

fn leaf_pixel(p: Vec2) -> [u8; 4] {
    if p.x > 0.99 {
        return [255; 4];
    }
    if p.x > 0.91 {
        // Original sinuous longitudinal ridges; the narrow atlas strip is
        // sampled by branches without adding a second texture lookup.
        let across = (p.x - 0.925) / 0.06 * std::f32::consts::TAU;
        let ridge = (across * 3.0 + (p.y * 12.0).sin() * 0.8).sin();
        let fissure = ((across * 7.0 + (p.y * 17.0).sin() * 0.7).sin().abs()).powi(12);
        let value = ((0.86 + 0.12 * ridge - 0.10 * fissure) * 255.0) as u8;
        return [value, value, value, 255];
    }
    if p.x >= 0.875 {
        return [255, 255, 255, 0];
    }
    let p = Vec2::new(p.x / 0.875, p.y);
    let mut result = [255, 255, 255, 0];
    for i in 0..29 {
        let seed = i * 179 + 31;
        let angle = random(seed) * std::f32::consts::TAU;
        let radius = random(seed ^ 733).sqrt() * 0.33;
        let center = Vec2::splat(0.5) + Vec2::from_angle(angle) * radius;
        let axis = Vec2::from_angle(angle + random(seed ^ 991) * 2.0);
        let delta = p - center;
        let width = 0.037 + random(seed ^ 613) * 0.023;
        let along = delta.dot(axis) / (width * 1.65);
        let across = delta.perp_dot(axis) / width;
        let edge = 1.0 - along * along - across * across;
        let alpha = (edge * 45.0 + 0.5).clamp(0.0, 1.0);
        if alpha * 255.0 > result[3] as f32 {
            // Baked per-leaf value variation and a very subtle center fold.
            let shade = 0.73 + 0.24 * random(seed ^ 1229) + 0.03 * (1.0 - across.abs()).max(0.0);
            result = [
                (shade * 255.0) as u8,
                (shade * 255.0) as u8,
                (shade * 255.0) as u8,
                (alpha * 255.0) as u8,
            ];
        }
    }
    result
}

pub(super) fn image() -> Image {
    let mut level: Vec<[u8; 4]> = (0..SIZE * SIZE)
        .map(|i| {
            leaf_pixel(Vec2::new((i % SIZE) as f32 + 0.5, (i / SIZE) as f32 + 0.5) / SIZE as f32)
        })
        .collect();
    let coverage = level.iter().filter(|p| p[3] >= 128).count() as f32 / level.len() as f32;
    let mut bytes = Vec::new();
    let mut size = SIZE;
    loop {
        bytes.extend(level.iter().flatten().copied());
        if size == 1 {
            break;
        }
        let next_size = size / 2;
        let mut next: Vec<[u8; 4]> = (0..next_size * next_size)
            .map(|pixel| {
                let origin = pixel / next_size * 2 * size + pixel % next_size * 2;
                std::array::from_fn(|channel| {
                    [origin, origin + 1, origin + size, origin + size + 1]
                        .into_iter()
                        .map(|i| u32::from(level[i][channel]))
                        .sum::<u32>()
                        .div_ceil(4) as u8
                })
            })
            .collect();
        // Preserve alpha-test area; regular averaging erases distant leaves.
        let (mut lo, mut hi) = (0.25, 4.0);
        for _ in 0..10 {
            let scale = (lo + hi) * 0.5;
            let visible = next.iter().filter(|p| p[3] as f32 * scale >= 128.0).count() as f32
                / next.len() as f32;
            if visible < coverage {
                lo = scale;
            } else {
                hi = scale;
            }
        }
        for pixel in &mut next {
            pixel[3] = (pixel[3] as f32 * hi).min(255.0) as u8;
        }
        level = next;
        size = next_size;
    }
    let mut image = Image::new_fill(
        Extent3d {
            width: SIZE as u32,
            height: SIZE as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[255; 4],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.mip_level_count = SIZE.ilog2() + 1;
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
    fn bark_wraps_without_a_vertical_color_seam() {
        for y in 0..SIZE {
            let y = (y as f32 + 0.5) / SIZE as f32;
            let left = leaf_pixel(Vec2::new(0.925, y));
            let right = leaf_pixel(Vec2::new(0.985, y));
            assert_eq!(left[3], 255);
            assert_eq!(right[3], 255);
            assert!(left[0].abs_diff(right[0]) <= 1);
        }
    }

    #[test]
    fn leaves_have_holes_and_an_opaque_bark_strip_at_a_fixed_small_budget() {
        let image = image();
        assert_eq!(image.texture_descriptor.mip_level_count, 9);
        let bytes = image.data.unwrap();
        assert_eq!(bytes.len(), 349_524);
        let leaf_area: Vec<_> = bytes[..SIZE * SIZE * 4]
            .chunks_exact(4)
            .enumerate()
            .filter(|(i, _)| i % SIZE < SIZE * 7 / 8)
            .collect();
        let coverage =
            leaf_area.iter().filter(|(_, p)| p[3] >= 128).count() as f32 / leaf_area.len() as f32;
        assert!((0.15..0.60).contains(&coverage), "leaf coverage {coverage}");
        for y in 0..SIZE {
            assert_eq!(bytes[(y * SIZE + 248) * 4 + 3], 255);
        }
    }
}
