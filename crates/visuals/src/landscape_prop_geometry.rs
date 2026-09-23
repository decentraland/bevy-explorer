//! Original small landscape assets. All meshes and colors are generated here.
use crate::tree_geometry::{random, Geometry};
use bevy::{prelude::*, render::mesh::VertexAttributeValues};

pub(super) const KINDS: usize = 6;

pub(super) fn mesh(kind: usize, low: bool) -> Mesh {
    match kind {
        0 | 1 => rock(kind, low),
        2 | 3 => bush(kind, low),
        _ => flowers(kind, low),
    }
}

fn rock(kind: usize, low: bool) -> Mesh {
    let sphere = Sphere::new(1.0).mesh().ico(u32::from(!low)).unwrap();
    let Some(VertexAttributeValues::Float32x3(points)) = sphere.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        unreachable!()
    };
    let points: Vec<_> = points
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
    let mut geometry = Geometry::default();
    let indices: Vec<_> = sphere.indices().unwrap().iter().collect();
    for (i, triangle) in indices.chunks_exact(3).enumerate() {
        let p = [
            points[triangle[0]],
            points[triangle[1]],
            points[triangle[2]],
        ];
        let n = (p[1] - p[0]).cross(p[2] - p[0]);
        if n.length_squared() < 0.000001 {
            continue;
        }
        let shade = 0.94 + 0.06 * random(i as u32 * 331 + kind as u32);
        let color = rock_color(n.normalize().y) * shade;
        geometry.triangle(p, color, false);
    }
    geometry.rigid_mesh()
}

fn rock_color(normal_y: f32) -> LinearRgba {
    // Moss caps whole upward-facing facets. Vertical faces remain grey;
    // the interpolation is linear-light, not a darkening sRGB blend.
    let stone = Color::srgb(0.59, 0.59, 0.54).to_linear();
    let moss = Color::srgb(0.043, 0.427, 0.302).to_linear();
    let bare = ((0.9 - normal_y) / (1.12 - normal_y)).clamp(0.0, 1.0);
    moss.mix(&stone, bare * bare * (3.0 - 2.0 * bare))
}

fn bush(kind: usize, low: bool) -> Mesh {
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
            if low { 2 } else { 1 },
            branch * 293 + kind as u32,
        );
    }
    geometry.mesh()
}

fn flowers(kind: usize, low: bool) -> Mesh {
    let mut geometry = Geometry::default();
    let petals = if kind == 4 {
        Color::srgb(0.91, 0.79, 0.63)
    } else {
        Color::srgb(0.44, 0.59, 0.60)
    }
    .to_linear();
    let leaf = Color::srgb(0.11, 0.31, 0.25).to_linear();
    for stem in 0..if low { 2 } else { 4 } {
        let angle = stem as f32 * 2.399;
        let root = Vec3::new(angle.cos() * 0.16, 0.0, angle.sin() * 0.16);
        let top = root
            + Vec3::new(
                0.06 * angle.cos(),
                0.27 + random(stem * 97 + kind as u32) * 0.25,
                0.08,
            );
        geometry.branch(root, top, 0.007, 0.003, 3);
        for side in [-1.0, 1.0] {
            let start = root.lerp(top, 0.4);
            let end = start + Vec3::new(side * 0.11, 0.06, side * 0.04);
            let across = Vec3::Z * 0.035;
            geometry.triangle([start, end - across, end], leaf, true);
            geometry.triangle([start, end, end + across], leaf, true);
        }
        for petal in 0..5 {
            let direction = Vec3::new(
                (petal as f32 * 1.2566).cos(),
                0.15,
                (petal as f32 * 1.2566).sin(),
            );
            let across = direction.cross(Vec3::Y) * 0.026;
            let end = top + direction * 0.075;
            let shoulder = top.lerp(end, 0.7) + Vec3::Y * 0.008;
            geometry.triangle([top, shoulder + across, end], petals, true);
            geometry.triangle([top, end, shoulder - across], petals, true);
        }
    }
    geometry.mesh()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moss_caps_upward_facets_without_darkening_every_stone_face() {
        let side = Srgba::from(rock_color(0.0));
        let top = Srgba::from(rock_color(1.0));
        assert!(side.red > 0.55 && side.green > 0.55 && side.blue > 0.50);
        assert!((side.red - side.green).abs() < 0.03);
        assert!(top.green > top.red * 5.0 && top.green > top.blue);
    }

    #[test]
    fn shared_props_are_finite_small_and_have_cheaper_distant_meshes() {
        for kind in 0..KINDS {
            let high = mesh(kind, false);
            let low = mesh(kind, true);
            assert!(low.count_vertices() < high.count_vertices());
            assert!(high.count_vertices() < 1200);
            if kind < 2 {
                assert_eq!(high.attributes().count(), 3);
                assert_eq!(low.attributes().count(), 3);
            }
            let Some(VertexAttributeValues::Float32x3(points)) =
                high.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                unreachable!()
            };
            assert!(points
                .iter()
                .all(|p| Vec3::from_array(*p).is_finite() && Vec3::from_array(*p).length() < 2.5));
        }
    }
}
