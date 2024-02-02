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

    fn shadow_material_key(&self, base_key: Option<u64>) -> Option<u64> {
        base_key.map(|_| {
            (((self.bounds.x as i64) << 32) | (self.bounds.y as i64)) as u64
        })
    }
}
pub struct SceneBoundPlugin;

impl Plugin for SceneBoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<SceneMaterial>::default());
    }
}
