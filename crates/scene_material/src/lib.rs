use bevy::{
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderRef},
};
use boimp::bake::{ImposterBakeMaterialExtension, ImposterBakeMaterialPlugin};
use common::structs::PreviewMode;

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
        let num_bounds = bounds.len() as u32;
        let bounds: [BoundRegion; 8] = if bounds.len() > 8 {
            warn!("super janky scene shape not supported");
            let overall_min = bounds.iter().fold(IVec2::MAX, |t, b| t.min(b.parcel_min()));
            let overall_max = bounds.iter().fold(IVec2::MIN, |t, b| t.max(b.parcel_max()));
            let overall_region = BoundRegion::new(overall_min, overall_max, bounds[0].parcel_count);
            [overall_region]
                .into_iter()
                .chain(std::iter::repeat(Default::default()))
                .take(8)
                .collect::<Vec<_>>()
                .try_into()
                .unwrap()
        } else {
            bounds
                .into_iter()
                .chain(std::iter::repeat(Default::default()))
                .take(8)
                .collect::<Vec<_>>()
                .try_into()
                .unwrap()
        };
        Self {
            data: SceneBoundData {
                num_bounds,
                bounds,
                distance,
                flags: 0,
                _pad: 0,
            },
        }
    }

    pub fn new_outlined(bounds: Vec<BoundRegion>, distance: f32, force_outline: bool) -> Self {
        Self {
            data: SceneBoundData {
                flags: SCENE_MATERIAL_OUTLINE
                    + if force_outline {
                        SCENE_MATERIAL_OUTLINE_FORCE
                    } else {
                        0
                    },
                ..Self::new(bounds, distance).data
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
                _pad: 0,
            },
        }
    }
}

mod decl {
    // temporary for ShaderType macro, remove in future
    #![allow(dead_code)]

    use bevy::render::render_resource::ShaderType;
    #[derive(ShaderType, Clone, Copy, Debug, Default)]
    pub struct BoundRegion {
        pub min: u32, // 2x i16
        pub max: u32, // 2x i16
        pub height: f32,
        pub parcel_count: u32,
    }

    #[derive(ShaderType, Clone)]
    pub struct SceneBoundData {
        pub(super) bounds: [BoundRegion; 8],
        pub distance: f32,
        pub flags: u32,
        pub num_bounds: u32,
        pub(super) _pad: u32,
    }
}
pub use decl::*;

impl BoundRegion {
    pub fn new(min: IVec2, max: IVec2, parcel_count: u32) -> Self {
        Self {
            min: ((min.x as i16 as u16 as u32) << 16) | (-(max.y + 1) as i16 as u16 as u32),
            max: (((max.x + 1) as i16 as u16 as u32) << 16) | (-min.y as i16 as u16 as u32),
            height: f32::log2(parcel_count as f32 + 1.0) * 20.0,
            parcel_count,
        }
    }
}

impl BoundRegion {
    fn unpack_parcel_coords(input: u32) -> IVec2 {
        let x = ((input >> 16) & 0xFFFF) as i32;
        let y = (input & 0xFFFF) as i32;
        IVec2::new(
            if (x & 0x8000) != 0 { x - 0x10000 } else { x },
            if (y & 0x8000) != 0 { y - 0x10000 } else { y },
        )
    }

    pub fn parcel_min(&self) -> IVec2 {
        IVec2::new(
            Self::unpack_parcel_coords(self.min).x,
            -Self::unpack_parcel_coords(self.max).y,
        )
    }

    pub fn parcel_max(&self) -> IVec2 {
        IVec2::new(
            Self::unpack_parcel_coords(self.max).x - 1,
            -Self::unpack_parcel_coords(self.min).y - 1,
        )
    }

    pub fn world_min(&self) -> Vec3 {
        let coords = Self::unpack_parcel_coords(self.min).as_vec2() * 16.0;
        Vec3::new(coords.x, 0.0, coords.y)
    }

    pub fn world_max(&self) -> Vec3 {
        let coords = Self::unpack_parcel_coords(self.max).as_vec2() * 16.0;
        Vec3::new(coords.x, self.height, coords.y)
    }

    pub fn world_size(&self) -> Vec3 {
        self.world_max() - self.world_min()
    }

    pub fn world_midpoint(&self) -> Vec3 {
        (self.world_max() + self.world_min()) * 0.5
    }

    pub fn world_radius(&self) -> f32 {
        (self.world_max() - self.world_min()).length() * 0.5
    }
}

#[cfg(test)]
mod test {
    use bevy::math::{IVec2, Vec3};

    use crate::BoundRegion;

    #[test]
    fn test_bounds() {
        for x in [-10, 0, 10] {
            for y in [-10, 0, 10] {
                let region = BoundRegion::new(IVec2::new(x, y), IVec2::new(x, y), 1);

                println!(
                    "[{},{}] -> {:x},{:x} -> {}, {}",
                    x,
                    y,
                    region.min,
                    region.max,
                    region.world_min(),
                    region.world_max()
                );
                assert_eq!(
                    region.world_min(),
                    Vec3::new(x as f32 * 16.0, 0.0, (-y - 1) as f32 * 16.0)
                );
                assert_eq!(
                    region.world_max(),
                    Vec3::new((x + 1) as f32 * 16.0, 20.0, -y as f32 * 16.0)
                );
                assert_eq!(region.parcel_min(), IVec2::new(x, y));
                assert_eq!(region.parcel_max(), IVec2::new(x, y));
            }
        }
    }
}

impl MaterialExtension for SceneBound {
    fn fragment_shader() -> bevy::render::render_resource::ShaderRef {
        ShaderRef::Path("embedded://shaders/bound_material.wgsl".into())
    }

    fn prepass_fragment_shader() -> ShaderRef {
        ShaderRef::Path("embedded://shaders/bound_prepass.wgsl".into())
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

impl ImposterBakeMaterialExtension for SceneBound {
    fn imposter_fragment_shader() -> bevy::render::render_resource::ShaderRef {
        "embedded://shaders/bound_material_baker.wgsl".into()
    }
}
pub struct SceneBoundPlugin;

impl Plugin for SceneBoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<SceneMaterial>::default());
        let preview_mode = app
            .world()
            .get_resource::<PreviewMode>()
            .is_some_and(|p| p.is_preview);
        if !preview_mode {
            app.add_plugins(ImposterBakeMaterialPlugin::<SceneMaterial>::default());
        }

        app.add_systems(Update, update_show_outside);
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
