use bevy::{
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderRef},
};

pub type SceneMaterial = ExtendedMaterial<StandardMaterial, SceneBound>;

#[derive(Asset, TypePath, Clone, AsBindGroup)]
pub struct SceneBound {
    #[uniform(100)]
    pub bounds: Vec4,
}

impl MaterialExtension for SceneBound {
    fn fragment_shader() -> bevy::render::render_resource::ShaderRef {
        ShaderRef::Path("shaders/bound_material.wgsl".into())
    }

    fn prepass_fragment_shader() -> ShaderRef {
        ShaderRef::Path("shaders/bound_prepass.wgsl".into())
    }
}

pub struct SceneBoundPlugin;

impl Plugin for SceneBoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<SceneMaterial>::default());
    }
}
