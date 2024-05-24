use bevy::{
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderRef, ShaderType},
};
use comms::preview::PreviewMode;

pub type SceneMaterial = ExtendedMaterial<StandardMaterial, SceneBound>;

pub const SCENE_MATERIAL_SHOW_OUTSIDE: u32 = 1;
pub const SCENE_MATERIAL_OUTLINE: u32 = 2;
pub const SCENE_MATERIAL_OUTLINE_RED: u32 = 4;
pub const SCENE_MATERIAL_OUTLINE_FORCE: u32 = 8;

pub trait SceneMaterialExt {
    fn unbounded_outlined(mat: StandardMaterial, force: bool) -> Self
    where
        Self: Sized;
}

impl SceneMaterialExt for SceneMaterial {
    fn unbounded_outlined(mat: StandardMaterial, force: bool) -> Self
    where
        Self: Sized,
    {
        Self {
            base: mat,
            extension: SceneBound::unbounded_outlined(force),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SceneBoundKey {
    outline: bool,
}

impl From<&SceneBound> for SceneBoundKey {
    fn from(value: &SceneBound) -> Self {
        Self {
            outline: (value.data.flags & SCENE_MATERIAL_OUTLINE) != 0,
        }
    }
}

#[derive(Asset, TypePath, Clone, AsBindGroup)]
#[bind_group_data(SceneBoundKey)]
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
                flags: 0,
            },
        }
    }

    pub fn unbounded_outlined(force_outline: bool) -> Self {
        Self {
            data: SceneBoundData {
                bounds: Vec4::new(
                    f32::NEG_INFINITY,
                    f32::NEG_INFINITY,
                    f32::INFINITY,
                    f32::INFINITY,
                ),
                distance: 0.0,
                flags: SCENE_MATERIAL_OUTLINE
                    + if force_outline {
                        SCENE_MATERIAL_OUTLINE_FORCE
                    } else {
                        0
                    },
            },
        }
    }
}

#[derive(ShaderType, Clone)]
pub struct SceneBoundData {
    pub bounds: Vec4,
    pub distance: f32,
    pub flags: u32,
}

impl MaterialExtension for SceneBound {
    fn fragment_shader() -> bevy::render::render_resource::ShaderRef {
        ShaderRef::Path("shaders/bound_material.wgsl".into())
    }

    fn prepass_fragment_shader() -> ShaderRef {
        ShaderRef::Path("shaders/bound_prepass.wgsl".into())
    }

    fn specialize(
        _: &bevy::pbr::MaterialExtensionPipeline,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        _: &bevy::render::mesh::MeshVertexBufferLayout,
        key: bevy::pbr::MaterialExtensionKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        let data = key.bind_group_data;
        if data.outline {
            if let Some(fragment) = descriptor.fragment.as_mut() {
                fragment.shader_defs.push("OUTLINE".into());
            }
        }
        Ok(())
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
                if (asset.extension.data.flags & SCENE_MATERIAL_SHOW_OUTSIDE) == 0 {
                    let asset = mats.get_mut(*id).unwrap();
                    asset.extension.data.flags |= SCENE_MATERIAL_SHOW_OUTSIDE;
                }
            }
        }
    } else {
        evs.read();
    }
}
