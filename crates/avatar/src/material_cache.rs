use bevy::prelude::*;
use scene_material::BoundRegion;

// cache keys for derived avatar materials. identical avatars should share
// material assets so they can batch: bindless is disabled, so every material
// asset is its own bind group and a batch break. keys hash exactly the fields
// the derived materials are built from, with f32s taken as raw bits.

pub type BoundsBits = Vec<(u32, u32, u32, u32)>;

#[derive(PartialEq, Eq, Hash, Clone)]
pub struct AvatarMatKey {
    source: Option<AssetId<StandardMaterial>>,
    texture: Option<AssetId<Image>>,
    base_color: [u32; 4],
    emissive: [u32; 4],
    depth_bias: u32,
    bounds: BoundsBits,
    oob: u32,
}

#[derive(PartialEq, Eq, Hash, Clone)]
pub struct AvatarMaskKey {
    texture: AssetId<Image>,
    mask: AssetId<Image>,
    color: [u32; 4],
    bounds: BoundsBits,
    oob: u32,
}

fn color_bits(color: Color) -> [u32; 4] {
    linear_bits(color.to_linear())
}

fn linear_bits(color: LinearRgba) -> [u32; 4] {
    color.to_vec4().to_array().map(f32::to_bits)
}

pub fn bounds_bits(bounds: &[BoundRegion]) -> BoundsBits {
    bounds
        .iter()
        .map(|b| (b.min, b.max, b.height.to_bits(), b.parcel_count))
        .collect()
}

impl AvatarMatKey {
    pub fn new(
        source: Option<AssetId<StandardMaterial>>,
        texture: Option<AssetId<Image>>,
        base_color: Color,
        emissive: LinearRgba,
        depth_bias: f32,
        bounds: BoundsBits,
        oob: f32,
    ) -> Self {
        Self {
            source,
            texture,
            base_color: color_bits(base_color),
            emissive: linear_bits(emissive),
            depth_bias: depth_bias.to_bits(),
            bounds,
            oob: oob.to_bits(),
        }
    }
}

impl AvatarMaskKey {
    pub fn new(
        texture: AssetId<Image>,
        mask: AssetId<Image>,
        color: Color,
        bounds: BoundsBits,
        oob: f32,
    ) -> Self {
        Self {
            texture,
            mask,
            color: color_bits(color),
            bounds,
            oob: oob.to_bits(),
        }
    }
}
