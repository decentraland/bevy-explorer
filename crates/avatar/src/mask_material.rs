use bevy::{
    prelude::*,
    reflect::{TypeUuid, TypePath},
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
#[derive(AsBindGroup, TypeUuid, Debug, Clone, TypePath)]
#[uuid = "10112ed8-3563-4886-91b8-53a4c95e3337"]
pub struct MaskMaterial {
    #[uniform(0)]
    pub color: Color,
    #[texture(1)]
    #[sampler(2)]
    pub base_texture: Handle<Image>,
    #[texture(3)]
    #[sampler(4)]
    pub mask_texture: Handle<Image>,
}
