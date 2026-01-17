use bevy::asset::{RenderAssetTransferPriority, RenderAssetUsages};
use bevy::math::{IVec2, Vec3, Vec3Swizzles};
use bevy::reflect::prelude::*;
use bevy::render::mesh::{Indices, Mesh, MeshBuilder, PrimitiveTopology};

use crate::imposter_spec::ImposterSpec;
use crate::render::SceneImposter;

#[derive(Clone, Copy, Debug, Reflect)]
#[reflect(Default, Debug, Clone)]
pub struct ImposterMesh {
    pub min: Vec3,
    pub max: Vec3,
    pub with_bake_attributes: bool,
}

impl Default for ImposterMesh {
    fn default() -> Self {
        Self {
            min: Vec3::splat(-0.5),
            max: Vec3::splat(0.5),
            with_bake_attributes: true,
        }
    }
}

impl ImposterMesh {
    pub fn from_spec(spec: &ImposterSpec, target: &SceneImposter) -> Mesh {
        let level_shift = 1 << target.level;
        let parcel_min = ((target.parcel + IVec2::Y * level_shift) * IVec2::new(16, -16)).as_vec2();
        let parcel_max = ((target.parcel + IVec2::X * level_shift) * IVec2::new(16, -16)).as_vec2();

        let spec_mid = ((spec.region_min + spec.region_max) / 2.0).xz();
        let spec_size = (spec.region_max - spec.region_min).xz();
        let effective_min = (parcel_min - spec_mid) / spec_size;
        let effective_max = (parcel_max - spec_mid) / spec_size;

        let builder = Self {
            min: Vec3::new(effective_min.x.max(-0.5), -0.5, effective_min.y.max(-0.5)),
            max: Vec3::new(effective_max.x.min(0.5), 0.5, effective_max.y.min(0.5)),
            with_bake_attributes: target.as_ingredient,
        };

        builder.build()
    }
}

impl MeshBuilder for ImposterMesh {
    fn build(&self) -> Mesh {
        let min = self.min;
        let max = self.max;

        let positions = vec![
            [min.x, min.y, min.z],
            [max.x, min.y, min.z],
            [min.x, max.y, min.z],
            [max.x, max.y, min.z],
            [min.x, min.y, max.z],
            [max.x, min.y, max.z],
            [min.x, max.y, max.z],
            [max.x, max.y, max.z],
        ];

        let indices = Indices::U16(vec![
            4, 5, 7, 7, 6, 4, 2, 3, 1, 1, 0, 2, 1, 3, 7, 7, 5, 1, 4, 6, 2, 2, 0, 4, 3, 2, 6, 6, 7,
            3,
            // 5, 4, 0, 0, 1, 5, // don't need a bottom
        ]);

        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );

        if self.with_bake_attributes {
            mesh = mesh
                .with_inserted_attribute(
                    Mesh::ATTRIBUTE_NORMAL,
                    positions
                        .iter()
                        .map(|p| Vec3::new(p[0], p[1], p[2]).normalize())
                        .collect::<Vec<_>>(),
                )
                .with_inserted_attribute(
                    Mesh::ATTRIBUTE_UV_0,
                    positions.iter().map(|p| [p[0], p[1]]).collect::<Vec<_>>(),
                );
        }

        mesh = mesh
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
            .with_inserted_indices(indices);
        mesh.transfer_priority = RenderAssetTransferPriority::Priority(-1);

        mesh
    }
}
