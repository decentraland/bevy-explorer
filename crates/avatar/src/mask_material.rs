use bevy::{
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, ShaderRef},
};

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

// This is the struct that will be passed to your shader
#[derive(AsBindGroup, Asset, Debug, Clone, TypePath)]
pub struct MaskMaterial {
    #[uniform(0)]
    pub color: Vec4,
    #[texture(1)]
    #[sampler(2)]
    pub base_texture: Handle<Image>,
    #[texture(3)]
    #[sampler(4)]
    pub mask_texture: Handle<Image>,
}
