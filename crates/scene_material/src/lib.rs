use bevy::{
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderRef, ShaderType},
};

pub type SceneMaterial = ExtendedMaterial<StandardMaterial, SceneBound>;

pub trait SceneMaterialExt {
    fn unbounded(mat: StandardMaterial) -> Self
    where
        Self: Sized;
}

impl SceneMaterialExt for SceneMaterial {
    fn unbounded(mat: StandardMaterial) -> Self
    where
        Self: Sized,
    {
        Self {
            base: mat,
            extension: SceneBound::unbounded(),
        }
    }
}

#[derive(Asset, TypePath, Clone, AsBindGroup)]
pub struct SceneBound {
    #[uniform(100)]
    pub data: SceneBoundData,
}

impl SceneBound {
    pub fn new(bounds: Vec4, distance: f32) -> Self {
        Self {
            data: SceneBoundData { bounds, distance },
        }
    }

    pub fn unbounded() -> Self {
        Self {
            data: SceneBoundData {
                bounds: Vec4::new(
                    f32::NEG_INFINITY,
                    f32::NEG_INFINITY,
                    f32::INFINITY,
                    f32::INFINITY,
                ),
                distance: 0.0,
            },
        }
    }
}

#[derive(ShaderType, Clone)]
pub struct SceneBoundData {
    pub bounds: Vec4,
    distance: f32,
}

impl MaterialExtension for SceneBound {
    fn fragment_shader() -> bevy::render::render_resource::ShaderRef {
        ShaderRef::Path("shaders/bound_material.wgsl".into())
    }

    fn prepass_fragment_shader() -> ShaderRef {
        ShaderRef::Path("shaders/bound_prepass.wgsl".into())
    }

    // fn shadow_material_key(&self, base_key: Option<u64>) -> Option<u64> {
    //     base_key.map(|_| (((self.bounds.x as i64) << 32) | (self.bounds.y as i64)) as u64)
    // }
}
pub struct SceneBoundPlugin;

impl Plugin for SceneBoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<SceneMaterial>::default());
    }
}
