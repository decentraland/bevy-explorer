//! Original periodic ripple slopes, generated once and filtered by the GPU.
use crate::tree_geometry::random;
use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

const SIZE: usize = 256;

fn noise(p: Vec2, cells: i32, seed: u32) -> f32 {
    let p = p * cells as f32;
    let cell = p.floor().as_ivec2();
    // Warped coordinates can cross zero: Rust's fract keeps the sign, unlike
    // a texture's repeating UVs. Subtract floor to preserve the wrapped seam.
    let f = p - p.floor();
    let f = f * f * f * (f * (f * 6.0 - Vec2::splat(15.0)) + Vec2::splat(10.0));
    let value = |x: i32, y: i32| {
        random(seed ^ (x.rem_euclid(cells) as u32 * 733) ^ (y.rem_euclid(cells) as u32 * 179))
    };
    let a = value(cell.x, cell.y).lerp(value(cell.x + 1, cell.y), f.x);
    let b = value(cell.x, cell.y + 1).lerp(value(cell.x + 1, cell.y + 1), f.x);
    a.lerp(b, f.y)
}

fn height_field(p: Vec2) -> f32 {
    // Bake gentle domain distortion once, not in every ocean fragment. The
    // whole field still wraps, but its ripples no longer sit on octave grids.
    let warp = Vec2::new(noise(p, 4, 1709), noise(p, 5, 2371)) - Vec2::splat(0.5);
    let p = p + warp * 0.11;
    noise(p, 7, 37) + noise(p, 13, 113) * 0.45 + noise(p, 29, 691) * 0.2 + noise(p, 61, 947) * 0.08
}

pub(super) fn image() -> Image {
    let heights: Vec<f32> = (0..SIZE * SIZE)
        .map(|i| {
            let p = Vec2::new((i % SIZE) as f32, (i / SIZE) as f32) / SIZE as f32;
            height_field(p)
        })
        .collect();
    let height = |x: usize, y: usize| heights[(y % SIZE) * SIZE + x % SIZE];
    let mut level: Vec<[u8; 2]> = (0..SIZE * SIZE)
        .map(|i| {
            let (x, y) = (i % SIZE, i / SIZE);
            let slope = Vec2::new(
                height(x + 1, y) - height(x + SIZE - 1, y),
                height(x, y + 1) - height(x, y + SIZE - 1),
            ) * 4.0;
            slope
                .to_array()
                .map(|v| ((v.clamp(-1.0, 1.0) * 0.5 + 0.5) * 255.0).round() as u8)
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
                    (([origin, origin + 1, origin + size, origin + size + 1]
                        .into_iter()
                        .map(|p| u32::from(level[p][channel]))
                        .sum::<u32>()
                        + 2)
                        / 4) as u8
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
        &[128; 2],
        TextureFormat::Rg8Unorm,
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
    fn ripple_texture_is_periodic_deterministic_and_small() {
        for cells in [4, 5, 7, 13, 29, 61] {
            for i in 0..=128 {
                let t = i as f32 / 128.0;
                assert_eq!(
                    noise(Vec2::new(0.0, t), cells, 37),
                    noise(Vec2::new(1.0, t), cells, 37)
                );
                assert_eq!(
                    noise(Vec2::new(t, 0.0), cells, 37),
                    noise(Vec2::new(t, 1.0), cells, 37)
                );
            }
        }
        let image = image();
        let bytes = image.data.unwrap();
        assert_eq!(bytes.len(), 174762);
        assert_eq!(image.texture_descriptor.mip_level_count, 9);
        assert_eq!(bytes, super::image().data.unwrap());
        assert!(bytes[..SIZE * SIZE * 2].iter().any(|&v| v < 100));
        assert!(bytes[..SIZE * SIZE * 2].iter().any(|&v| v > 155));
        assert!(bytes[bytes.len() - 2..]
            .iter()
            .all(|v| (125..=131).contains(v)));
    }

    #[test]
    fn warped_height_field_wraps_smoothly_even_across_negative_coordinates() {
        let epsilon = 0.0001;
        for i in 0..=128 {
            let p = Vec2::new(-0.25, i as f32 / 128.0);
            for offset in [Vec2::X, Vec2::Y] {
                assert!((height_field(p) - height_field(p + offset)).abs() < 0.00005);
                let before = height_field(p - offset * epsilon);
                let wrapped_before = height_field(p + offset * (1.0 - epsilon));
                let after = height_field(p + offset * epsilon);
                let wrapped_after = height_field(p + offset * (1.0 + epsilon));
                assert!((before - wrapped_before).abs() < 0.00005);
                assert!((after - wrapped_after).abs() < 0.00005);
            }
        }
    }

    #[test]
    fn ocean_warps_chop_by_swells_without_more_texture_reads_or_passes() {
        let shader = include_str!("landscape_water.wgsl");
        assert_eq!(shader.matches("textureSample(").count(), 2);
        assert!(shader.contains("+ swell * 0.72"));
        assert!(shader.contains("let chop_strength ="));
        assert!(shader.contains("swell_space * vec2(0.0073, 0.0091)"));
        assert!(
            !shader.contains("textureSampleLevel"),
            "keep derivative-selected mip filtering"
        );
    }
}
