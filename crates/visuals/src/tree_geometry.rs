//! Original procedural tree meshes; no imported models or foliage textures.
use bevy::{
    asset::RenderAssetUsages,
    math::Vec3A,
    prelude::*,
    render::mesh::{Indices, PrimitiveTopology},
    render::{mesh::MeshAabb, primitives::Aabb},
};

pub(super) const KINDS: usize = 3;
pub(super) const CROWN_RADIUS: f32 = 4.4;

mod branches;

// Props retain their original shared bound across all of their LODs.
pub(super) fn lod_bounds(meshes: &[Mesh], wind: f32) -> Aabb {
    let mut min = Vec3A::splat(f32::INFINITY);
    let mut max = Vec3A::splat(f32::NEG_INFINITY);
    for mesh in meshes {
        let bounds = mesh.compute_aabb().expect("landscape mesh has positions");
        min = min.min(bounds.min());
        max = max.max(bounds.max());
    }
    Aabb::from_min_max(
        (min - Vec3A::splat(wind)).into(),
        (max + Vec3A::splat(wind)).into(),
    )
}

// Keep each mesh's own full bounds: low-detail sprays are wider, while higher
// detail includes more sprays. The caller changes mesh and AABB together.
pub(super) fn tree_lod_bounds(meshes: &[Mesh; 3], wind: f32) -> [Aabb; 3] {
    std::array::from_fn(|lod| {
        let mesh = &meshes[lod];
        let bounds = mesh.compute_aabb().expect("landscape mesh has positions");
        Aabb::from_min_max(
            (bounds.min() - Vec3A::splat(wind)).into(),
            (bounds.max() + Vec3A::splat(wind)).into(),
        )
    })
}

pub(super) fn random(seed: u32) -> f32 {
    let mut x = seed.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    x = ((x >> ((x >> 28) + 4)) ^ x).wrapping_mul(277_803_737);
    ((x >> 22) ^ x) as f32 / u32::MAX as f32
}

#[derive(Default)]
pub(super) struct Geometry {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    uv: Vec<[f32; 2]>,
    weights: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

impl Geometry {
    pub(super) fn triangle(&mut self, points: [Vec3; 3], color: LinearRgba, foliage: bool) {
        let normal = (points[1] - points[0])
            .cross(points[2] - points[0])
            .normalize();
        let shade = if foliage {
            0.85 + 0.15 * normal.y + 0.06 * normal.x
        } else {
            1.0
        };
        for point in points {
            self.indices.push(self.positions.len() as u32);
            self.positions.push(point.to_array());
            self.normals.push(normal.to_array());
            self.colors.push([
                color.red * shade,
                color.green * shade,
                color.blue * shade,
                1.0,
            ]);
            // Opaque strip in the shared atlas. Leaves use the other 7/8ths.
            self.uv.push([0.997, 0.5]);
            self.weights.push([
                (point.y / 10.0).clamp(0.0, 1.0),
                if foliage { 1.0 } else { 0.0 },
            ]);
        }
    }

    pub(super) fn crown(&mut self, center: Vec3, size: Vec3, color: Color, lod: usize, seed: u32) {
        // Irregular leaf sprays through a volume, not a solid sphere with a
        // texture on it. One material/atlas and four vertices per spray.
        let count = [52, 28, 14][lod];
        for leaf in 0..count {
            let seed = seed.wrapping_add(leaf * 197);
            let y = random(seed ^ 91) * 2.0 - 1.0;
            let angle = random(seed ^ 523) * std::f32::consts::TAU;
            let radial = (1.0 - y * y).sqrt();
            let outward = Vec3::new(radial * angle.cos(), y, radial * angle.sin());
            let p = center + outward * size * (0.25 + random(seed ^ 613) * 0.55);
            let orientation = Quat::from_euler(
                EulerRot::YXZ,
                random(seed ^ 717) * std::f32::consts::TAU,
                (random(seed ^ 811) - 0.5) * 2.5,
                random(seed ^ 919) * std::f32::consts::TAU,
            );
            let radius = [0.64, 0.83, 1.08][lod]
                * (0.8 + random(seed ^ 1013) * 0.4)
                * size.max_element().min(1.0);
            let right = orientation * Vec3::X * radius;
            let up = orientation * Vec3::Y * radius;
            let normal = (outward + Vec3::Y * 0.75).normalize();
            let tint = color.to_linear();
            let shade = 0.78 + 0.3 * random(seed ^ 1117) + 0.12 * y.max(0.0);
            let base = self.positions.len() as u32;
            for (delta, uv) in [
                (-right - up, [0.0, 1.0]),
                (right - up, [0.875, 1.0]),
                (right + up, [0.875, 0.0]),
                (-right + up, [0.0, 0.0]),
            ] {
                let point = p + delta;
                self.positions.push(point.to_array());
                self.normals.push(normal.to_array());
                self.colors
                    .push([tint.red * shade, tint.green * shade, tint.blue * shade, 1.0]);
                self.uv.push(uv);
                self.weights.push([(point.y / 10.0).clamp(0.0, 1.0), 1.0]);
            }
            self.indices
                .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }

    pub(super) fn mesh(self) -> Mesh {
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::all())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uv)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, self.weights)
            .with_inserted_indices(Indices::U32(self.indices))
    }

    pub(super) fn rigid_mesh(self) -> Mesh {
        self.mesh()
            .with_removed_attribute(Mesh::ATTRIBUTE_UV_0)
            .with_removed_attribute(Mesh::ATTRIBUTE_UV_1)
    }
}

pub(super) fn tree_mesh(kind: usize, lod: usize) -> Mesh {
    let mut geometry = tree_geometry(kind, lod);
    // All tree LODs are far below 65,536 vertices. Keep the exact topology and
    // attributes while halving index fetch/storage; other procedural meshes
    // and the merged world-space trunk collider retain their existing format.
    let indices = std::mem::take(&mut geometry.indices)
        .into_iter()
        .map(|index| u16::try_from(index).expect("tree index fits u16"))
        .collect();
    geometry.mesh().with_inserted_indices(Indices::U16(indices))
}

fn tree_geometry(kind: usize, lod: usize) -> Geometry {
    let mut geometry = Geometry::default();
    let sides = [8, 6, 4][lod];
    let (height, crown, lean, color) = match kind {
        0 => (
            6.2,
            Vec3::new(1.85, 1.9, 1.65),
            Vec3::new(-0.48, 0.0, 0.38),
            Color::srgb(0.72, 0.20, 0.22),
        ),
        1 => (
            6.65,
            Vec3::new(1.3, 2.05, 1.3),
            Vec3::new(0.38, 0.0, -0.3),
            Color::srgb(0.92, 0.62, 0.19),
        ),
        _ => (
            5.9,
            Vec3::new(1.8, 1.65, 1.9),
            Vec3::new(-0.32, 0.0, -0.42),
            Color::srgb(0.09, 0.53, 0.34),
        ),
    };
    let bend = Vec3::new(0.65, 3.0, 0.13);
    let top = Vec3::Y * height + lean;
    let mut stem = trunk_points().to_vec();
    stem.extend([bend.lerp(top, 0.48) + Vec3::new(0.14, 0.0, -0.12), top]);
    geometry.branch_path(&stem, &[0.48, 0.36, 0.29, 0.24, 0.15, 0.07], sides);
    geometry.crown(top + Vec3::Y * 0.2, crown, color, lod, kind as u32 * 231);
    for branch in 0..6 {
        // Stagger the same six branches up the stem instead of building a
        // horizontal six-spoke umbrella. Golden-angle spacing and unequal
        // reach make overlapping lobes, with light passing between tiers.
        let angle = branch as f32 * 2.399_963 + kind as f32 * 0.7;
        let lift = random(branch + kind as u32 * 41);
        let tier = branch as f32 / 5.0;
        let reach = (1.25 + lift * 0.55) * (1.0 - tier * 0.25);
        let end = Vec3::new(
            angle.cos() * reach,
            height - 2.25 + tier * 2.1 + lift * 0.3,
            angle.sin() * reach,
        ) + lean * (0.2 + tier * 0.45);
        geometry.branch(
            bend.lerp(top, 0.08 + tier * 0.64),
            end,
            0.17 - tier * 0.045,
            0.035,
            sides,
        );
        geometry.crown(
            end,
            crown * (0.7 + lift * 0.13 - tier * 0.08),
            color,
            lod,
            branch * 997 + kind as u32 * 73,
        );
    }
    geometry
}

pub(super) fn trunk_collision() -> Mesh {
    let mut geometry = Geometry::default();
    geometry.branch_path(&trunk_points(), &[0.48, 0.36, 0.29, 0.24], 8);
    geometry.mesh()
}

fn trunk_points() -> [Vec3; 4] {
    [
        Vec3::new(0.0, -0.15, 0.0),
        Vec3::new(0.12, 1.0, -0.08),
        Vec3::new(0.43, 2.0, 0.02),
        Vec3::new(0.65, 3.0, 0.13),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::render::mesh::VertexAttributeValues;

    #[test]
    fn all_tree_lods_are_finite_bounded_and_progressively_simpler() {
        for kind in 0..KINDS {
            let mut previous = usize::MAX;
            for lod in 0..3 {
                let mesh = tree_mesh(kind, lod);
                assert!(mesh.count_vertices() < previous);
                // Original cap-per-segment tubes cost 2512 / 1576 / 920
                // vertices including these same leaf sprays.
                assert_eq!(mesh.count_vertices(), [1758, 1022, 566][lod]);
                previous = mesh.count_vertices();
                let Some(VertexAttributeValues::Float32x3(points)) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                else {
                    unreachable!()
                };
                assert!(points.iter().all(|point| {
                    let point = Vec3::from_array(*point);
                    point.is_finite() && point.xz().length() < CROWN_RADIUS && point.y < 10.0
                }));
                let Some(VertexAttributeValues::Float32x3(normals)) =
                    mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
                else {
                    unreachable!()
                };
                assert!(normals
                    .iter()
                    .all(|n| (Vec3::from_array(*n).length() - 1.0).abs() < 0.001));
                assert!(mesh
                    .indices()
                    .unwrap()
                    .iter()
                    .all(|i| i < mesh.count_vertices()));
            }
        }
    }

    #[test]
    fn every_lod_fits_its_visibility_bounds_including_wind() {
        for kind in 0..KINDS {
            let models: [Mesh; 3] = std::array::from_fn(|lod| tree_mesh(kind, lod));
            let bounds = tree_lod_bounds(&models, 0.4);
            for (mesh, bounds) in models.iter().zip(bounds) {
                let aabb = mesh.compute_aabb().unwrap();
                assert!((aabb.min() - bounds.min()).cmpge(Vec3A::splat(0.399)).all());
                assert!((bounds.max() - aabb.max()).cmpge(Vec3A::splat(0.399)).all());
            }
        }
    }

    #[test]
    fn crowns_are_staggered_and_shared_geometry_is_deterministic() {
        for kind in 0..KINDS {
            let mesh = tree_mesh(kind, 0);
            let repeated = tree_mesh(kind, 0);
            let Some(VertexAttributeValues::Float32x3(points)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                unreachable!()
            };
            let Some(VertexAttributeValues::Float32x3(other)) =
                repeated.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                unreachable!()
            };
            assert_eq!(points, other);
            let Some(VertexAttributeValues::Float32x2(weights)) =
                mesh.attribute(Mesh::ATTRIBUTE_UV_1)
            else {
                unreachable!()
            };
            let leaves: Vec<_> = points
                .iter()
                .zip(weights)
                .filter_map(|(point, weight)| {
                    (weight[1] == 1.0).then_some(Vec3::from_array(*point))
                })
                .collect();
            assert_eq!(leaves.len(), 7 * 52 * 4);
            let centers: Vec<_> = leaves
                .chunks_exact(52 * 4)
                .map(|crown| crown.iter().copied().sum::<Vec3>() / crown.len() as f32)
                .collect();
            let lowest = centers.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
            let highest = centers
                .iter()
                .map(|p| p.y)
                .fold(f32::NEG_INFINITY, f32::max);
            assert!(
                highest - lowest > 1.8,
                "crowns must not return to one flat tier"
            );
            assert!(highest > 5.8, "retain the taller silhouette");
        }
    }

    #[test]
    fn branch_side_normals_point_outward_and_uvs_stay_in_the_bark_strip() {
        let mut geometry = Geometry::default();
        geometry.branch(Vec3::ZERO, Vec3::Y * 2.0, 0.4, 0.2, 8);
        for ring in 0..2 {
            for index in ring * 9..ring * 9 + 9 {
                let position = Vec3::from_array(geometry.positions[index]);
                let normal = Vec3::from_array(geometry.normals[index]);
                assert!(position.xz().dot(normal.xz()) > 0.1);
                assert!((0.92..0.99).contains(&geometry.uv[index][0]));
            }
        }
    }
}
