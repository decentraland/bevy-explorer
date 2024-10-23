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
    pub fn new(bounds: Vec<BoundRegion>, distance: f32) -> Self {
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
            data: SceneBoundData {
                num_bounds,
                bounds,
                distance,
                flags: 0,
            },
        }
    }

    pub fn unbounded_outlined(force_outline: bool) -> Self {
        Self {
            data: SceneBoundData {
                num_bounds: 0,
                bounds: Default::default(),
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

#[derive(ShaderType, Clone, Debug, Default)]
pub struct BoundRegion {
    pub min: u32, // 2x i16
    pub max: u32, // 2x i16
    pub height: f32,
    _padding0: u32,
}

impl BoundRegion {
    pub fn new(min: IVec2, max: IVec2, parcel_count: usize) -> Self {
        Self {
            min: (min.x as i16 as u16 as u32) << 16 | (-(max.y + 1) as i16 as u16 as u32),
            max: ((max.x + 1) as i16 as u16 as u32) << 16 | (-min.y as i16 as u16 as u32),
            height: f32::log2(parcel_count as f32 + 1.0) * 20.0,
            ..Default::default()
        }
    }
}

#[derive(ShaderType, Clone)]
pub struct SceneBoundData {
    bounds: [BoundRegion; 8],
    pub distance: f32,
    pub flags: u32,
    pub num_bounds: u32,
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
        _: &bevy::render::mesh::MeshVertexBufferLayoutRef,
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
