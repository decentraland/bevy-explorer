use bevy::{
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, ShaderRef},
};
use scene_material::BoundRegion;

pub struct MaskMaterialPlugin;

impl Plugin for MaskMaterialPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<MaskMaterial>::default());
    }
}

impl Material for MaskMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://shaders/mask_material.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }

    fn depth_bias(&self) -> f32 {
        10000.0
    }
}

mod decl {
    #![allow(dead_code)]
    use bevy::{math::Vec4, render::render_resource::ShaderType};

    #[derive(ShaderType, Debug, Clone)]
    pub struct MaskData {
        pub(super) bounds: [scene_material::BoundRegion; 8],
        pub(super) color: Vec4,
        pub(super) distance: f32,
        pub(super) num_bounds: u32,
        pub(super) _pad: u32,
    }
}
use decl::*;

// This is the struct that will be passed to your shader
#[derive(AsBindGroup, Asset, Debug, Clone, TypePath)]
pub struct MaskMaterial {
    #[uniform(0)]
    pub mask_data: MaskData,
    #[texture(1)]
    #[sampler(2)]
    pub base_texture: Handle<Image>,
    #[texture(3)]
    #[sampler(4)]
    pub mask_texture: Handle<Image>,
}

impl MaskMaterial {
    pub fn new(
        color: Color,
        base_texture: Handle<Image>,
        mask_texture: Handle<Image>,
        bounds: Vec<BoundRegion>,
        distance: f32,
    ) -> Self {
        let num_bounds = bounds.len() as u32;
        let bounds: [BoundRegion; 8] = if bounds.len() > 8 {
            warn!("super janky scene shape not supported");
            let overall_min = bounds.iter().fold(IVec2::MAX, |t, b| t.min(b.parcel_min()));
            let overall_max = bounds.iter().fold(IVec2::MIN, |t, b| t.max(b.parcel_max()));
            let overall_region = BoundRegion::new(overall_min, overall_max, bounds[0].parcel_count);
            [overall_region]
                .into_iter()
                .chain(std::iter::repeat(Default::default()))
                .take(8)
                .collect::<Vec<_>>()
                .try_into()
                .unwrap()
        } else {
            bounds
                .into_iter()
                .chain(std::iter::repeat(Default::default()))
                .take(8)
                .collect::<Vec<_>>()
                .try_into()
                .unwrap()
        };
        Self {
            mask_data: MaskData {
                num_bounds,
                color: color.to_linear().to_vec4(),
                bounds,
                distance,
                _pad: 0,
            },
            base_texture,
            mask_texture,
        }
    }
}
