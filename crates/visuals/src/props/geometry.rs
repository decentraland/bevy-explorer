//! Original small landscape assets. All meshes and colors are generated here.
use bevy::{prelude::*, render::mesh::VertexAttributeValues};

use crate::trees::geometry::{random, Geometry};

pub(super) const ROCKS: usize = 2;
pub(super) const KINDS: usize = 4;
/// Rock sizes, in steps of `ROCK_SCALE_STEP` from `ROCK_MIN_SCALE`, so rocks can share collision
/// shapes.
pub(super) const ROCK_SCALES: usize = 17;
const ROCK_MIN_SCALE: f32 = 0.65;
const ROCK_SCALE_STEP: f32 = 0.1;

/// The rock size nearest `scale`.
pub(super) fn rock_size(scale: f32) -> usize {
    (((scale - ROCK_MIN_SCALE) / ROCK_SCALE_STEP).round() as usize).min(ROCK_SCALES - 1)
}

pub(super) fn rock_scale(size: usize) -> f32 {
    ROCK_MIN_SCALE + size as f32 * ROCK_SCALE_STEP
}

/// Rocks are kinds `0..ROCKS`, bushes the rest.
pub(super) fn mesh(kind: usize) -> Mesh {
    if kind < ROCKS {
        rock(kind)
    } else {
        bush(kind)
    }
}

/// Rock surface points: an icosphere, flattened and made irregular.
fn rock_points(kind: usize) -> (Mesh, Vec<Vec3>) {
    let sphere = Sphere::new(1.0).mesh().ico(1).unwrap();
    let Some(VertexAttributeValues::Float32x3(points)) = sphere.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        unreachable!()
    };
    let points = points
        .iter()
        .map(|p| {
            let p = Vec3::from_array(*p);
            let irregular = 0.84 + 0.16 * (p.x * 11.0 + p.z * 7.0 + kind as f32).sin();
            Vec3::new(
                p.x * irregular,
                p.y.max(-0.25) * (0.65 + kind as f32 * 0.35),
                p.z * irregular * 0.87,
            )
        })
        .collect();
    (sphere, points)
}

fn rock(kind: usize) -> Mesh {
    let (sphere, points) = rock_points(kind);
    let stone = Color::srgb(0.59, 0.59, 0.54).to_linear();
    let mut geometry = Geometry::default();
    let indices: Vec<_> = sphere.indices().unwrap().iter().collect();
    for (i, triangle) in indices.chunks_exact(3).enumerate() {
        let p = [
            points[triangle[0]],
            points[triangle[1]],
            points[triangle[2]],
        ];
        if (p[1] - p[0]).cross(p[2] - p[0]).length_squared() < 0.000001 {
            continue;
        }
        let shade = 0.94 + 0.06 * random(i as u32 * 331 + kind as u32);
        geometry.triangle(p, stone * shade);
    }
    geometry.rigid_mesh()
}

fn bush(kind: usize) -> Mesh {
    let mut geometry = Geometry::default();
    let color = if kind == 2 {
        Color::srgb(0.08, 0.47, 0.34)
    } else {
        Color::srgb(0.16, 0.60, 0.37)
    };
    for branch in 0..3 {
        let angle = branch as f32 * 2.399;
        let top = Vec3::new(
            angle.cos() * 0.37,
            0.7 + 0.2 * random(branch * 127),
            angle.sin() * 0.37,
        );
        geometry.branch(Vec3::ZERO, top, 0.028, 0.01, 4);
        geometry.crown(
            top,
            Vec3::new(0.55, 0.45, 0.55),
            color,
            28,
            0.83,
            branch * 293 + kind as u32,
        );
    }
    geometry.mesh()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn props_are_finite_and_small() {
        for kind in 0..KINDS {
            let mesh = mesh(kind);
            assert!(mesh.count_vertices() < 1200);
            if kind < ROCKS {
                assert_eq!(mesh.attributes().count(), 3);
            }
            let Some(VertexAttributeValues::Float32x3(points)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                unreachable!()
            };
            assert!(points
                .iter()
                .all(|p| Vec3::from_array(*p).is_finite() && Vec3::from_array(*p).length() < 2.5));
        }
    }

    #[test]
    fn rock_sizes_cover_the_placed_scales() {
        assert_eq!(rock_size(0.0), 0);
        assert_eq!(rock_scale(rock_size(0.65)), 0.65);
        assert!((rock_scale(rock_size(2.2)) - 2.25).abs() < 0.001);
        assert_eq!(rock_size(10.0), ROCK_SCALES - 1);
        for size in 0..ROCK_SCALES {
            assert_eq!(rock_size(rock_scale(size)), size);
        }
    }
}
