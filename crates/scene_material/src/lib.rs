use bevy::{
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderRef, ShaderType},
};
use comms::preview::PreviewMode;

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
            data: SceneBoundData {
                bounds,
                distance,
                show_outside: 0,
            },
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
                show_outside: 0,
            },
        }
    }
}

#[derive(ShaderType, Clone)]
pub struct SceneBoundData {
    pub bounds: Vec4,
    distance: f32,
    show_outside: u32,
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
        app.add_plugins(MaterialPlugin::<SceneMaterial>::default())
            .add_systems(Update, update_show_outside);
    }
}

fn update_show_outside(
    preview: Res<PreviewMode>,
    mut mats: ResMut<Assets<SceneMaterial>>,
    mut evs: EventReader<AssetEvent<SceneMaterial>>,
) {
    if preview.is_preview {
        for ev in evs.read() {
            if let AssetEvent::Added { id } | AssetEvent::Modified { id } = ev {
                let Some(asset) = mats.get(*id) else {
                    continue;
                };
                if asset.extension.data.show_outside == 0 {
                    let asset = mats.get_mut(*id).unwrap();
                    asset.extension.data.show_outside = 1;
                }
            }
        }
    } else {
        evs.read();
    }
}
