use bevy::{
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderRef},
};

#[derive(SystemSet, Hash, PartialEq, Eq, PartialOrd, Ord, Debug, Clone)]
pub struct Ui9SliceSet;

pub struct StretchUvsImagePlugin;

impl Plugin for StretchUvsImagePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiMaterialPlugin::<StretchUvMaterial>::default());
    }
}

#[derive(AsBindGroup, Asset, TypePath, Debug, Clone)]
pub struct StretchUvMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub image: Handle<Image>,
    #[uniform(2)]
    pub uvs: [Vec4; 2],
    #[uniform(3)]
    pub color: Vec4,
}

impl UiMaterial for StretchUvMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/stretch_uv_material.wgsl".into()
    }
}
