use bevy::{
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, ShaderRef, ShaderType},
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
        "shaders/mask_material.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
}

#[derive(ShaderType, Debug, Clone)]
pub struct MaskData {
    bounds: [BoundRegion; 8],
    color: Vec4,
    distance: f32,
    num_bounds: u32,
}

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
        if bounds.len() > 8 {
            warn!("super janky scene shape not supported");
        }
        let num_bounds = bounds.len() as u32;
        let bounds: [BoundRegion; 8] = bounds
            .into_iter()
            .chain(std::iter::repeat(Default::default()))
            .take(8)
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        Self {
            mask_data: MaskData {
                num_bounds,
                color: color.to_linear().to_vec4(),
                bounds,
                distance,
            },
            base_texture,
            mask_texture,
        }
    }
}
