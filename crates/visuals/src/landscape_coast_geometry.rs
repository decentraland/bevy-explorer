use crate::{
    coast_profile::coast_profile,
    tree_geometry::{random, Geometry},
};
use bevy::{prelude::*, render::mesh::VertexAttributeValues};
use common::terrain::TerrainField;

pub(super) const SEA_LEVEL: f32 = -19.0;
pub(super) const CHUNK: f32 = 64.0;
const LAYERS: usize = 8;
// Ten facets per complete corner keep waterline chord error below 11 cm at the
// largest radius. Each side owns half, sharing an exact 45-degree endpoint.
const CORNER_STEPS: i32 = 5;

pub(super) fn bounds(field: &TerrainField) -> Vec4 {
    Vec4::new(
        field.min_parcel[0] as f32 * 16.0,
        field.min_parcel[1] as f32 * 16.0,
        (field.max_parcel[0] + 1) as f32 * 16.0,
        (field.max_parcel[1] + 1) as f32 * 16.0,
    )
}

// Counter-clockwise around Unity X/Z land bounds; conversion to Bevy flips Z.
fn edge(bounds: Vec4, side: u32, along: f32) -> (Vec2, Vec2) {
    let point = match side {
        0 => Vec2::new((bounds.x + along).min(bounds.z), bounds.y),
        1 => Vec2::new(bounds.z, (bounds.y + along).min(bounds.w)),
        2 => Vec2::new((bounds.z - along).max(bounds.x), bounds.w),
        _ => Vec2::new(bounds.x, (bounds.w - along).max(bounds.y)),
    };
    // Miter both adjoining edges at each corner: no holes in the coastline.
    let outward = Vec2::new(
        if point.x == bounds.x {
            -1.0
        } else if point.x == bounds.z {
            1.0
        } else {
            0.0
        },
        if point.y == bounds.y {
            -1.0
        } else if point.y == bounds.w {
            1.0
        } else {
            0.0
        },
    );
    (point, outward)
}

pub(super) fn side_length(bounds: Vec4, side: u32) -> f32 {
    if side.is_multiple_of(2) {
        bounds.z - bounds.x
    } else {
        bounds.w - bounds.y
    }
}

pub(super) fn near_chunks(bounds: Vec4, camera: Vec3) -> Vec<(u32, i32)> {
    let camera = Vec2::new(camera.x, -camera.z);
    let mut chunks = Vec::new();
    for side in 0..4 {
        let length = side_length(bounds, side);
        let (start, _) = edge(bounds, side, 0.0);
        let (end, _) = edge(bounds, side, length);
        let along = (camera - start)
            .dot((end - start).normalize())
            .clamp(0.0, length);
        let closest = start.lerp(end, along / length);
        if camera.distance(closest) > 768.0 {
            continue;
        }
        let first = ((along - 768.0).max(0.0) / CHUNK).floor() as i32;
        let last = (((along + 768.0).min(length) / CHUNK).ceil() as i32 - 1).max(first);
        for chunk in first..=last {
            chunks.push((side, chunk));
        }
    }
    chunks
}

fn arc_normal(side: u32, step: i32) -> Vec2 {
    let normal = [Vec2::NEG_Y, Vec2::X, Vec2::Y, Vec2::NEG_X][side as usize];
    let tangent = Vec2::new(-normal.y, normal.x);
    if step.abs() == CORNER_STEPS {
        // Do not separately evaluate sin/cos at the shared endpoint: adjacent
        // halves must produce bit-identical coordinates, not a one-ulp crack.
        return (normal + tangent * step.signum() as f32) * std::f32::consts::FRAC_1_SQRT_2;
    }
    let angle = step as f32 * (std::f32::consts::FRAC_PI_4 / CORNER_STEPS as f32);
    let (sine, cosine) = angle.sin_cos();
    normal * cosine + tangent * sine
}

fn profile(field: &TerrainField, edge: Vec2, outward: Vec2) -> [Vec3; LAYERS] {
    let bounds = bounds(field);
    let seed = (edge.x as i32 as u32).wrapping_mul(1337) ^ (edge.y as i32 as u32).wrapping_mul(733);
    // Every arc column shares the same anchor, height and radii. Inward sampling
    // depends on the core rectangle, not which side owns this half of a corner.
    let sample = edge.clamp(
        bounds.xy() + Vec2::splat(0.001),
        bounds.zw() - Vec2::splat(0.001),
    );
    let terrain = field.height(sample.x, sample.y);
    let radii = coast_profile(edge);
    let (crest, shore) = (radii.x, radii.y);
    let toe = (shore - crest - 18.0) * 0.4;
    let ledge = ((crest - 6.0) / 7.0).clamp(0.0, 1.0);
    let offsets = [
        0.0,
        crest,
        crest + 1.8 + ledge * 2.2,
        crest + 4.2 + ledge * 0.7,
        crest + 5.7 + toe,
        crest + 7.5 + toe,
        shore,
        shore + 6.0,
    ];
    let heights = [
        terrain,
        terrain - 0.15,
        -6.2 + ledge * 1.8,
        -11.5,
        -16.4,
        -17.3,
        SEA_LEVEL,
        SEA_LEVEL - 4.0,
    ];
    std::array::from_fn(|layer| {
        let noise = random(seed ^ (layer as u32).wrapping_mul(971));
        let fracture = (2..5).contains(&layer);
        let offset = offsets[layer] + if fracture { (noise - 0.5) * 0.8 } else { 0.0 };
        let height = heights[layer]
            - if fracture {
                noise * if layer == 4 { 0.7 } else { 1.3 }
            } else {
                0.0
            };
        let horizontal = edge + outward * offset;
        Vec3::new(horizontal.x, height, -horizontal.y)
    })
}

fn columns(field: &TerrainField, side: u32, chunk: i32) -> Vec<[Vec3; LAYERS]> {
    let bounds = bounds(field);
    let length = side_length(bounds, side);
    let start = chunk as f32 * CHUNK;
    let end = (start + CHUNK).min(length);
    let mut segments = ((end - start) / 4.0).ceil() as usize;
    // A tiny realm can put both rounded ends in one chunk. Keep its total under
    // 900 vertices too; normal chunks retain the original four-metre sampling.
    if start == 0.0 && end == length {
        segments = segments.min(12);
    }
    let mut columns = Vec::with_capacity(segments + 1 + 2 * CORNER_STEPS as usize);
    if start == 0.0 {
        let anchor = edge(bounds, side, 0.0).0;
        for step in -CORNER_STEPS..0 {
            columns.push(profile(field, anchor, arc_normal(side, step)));
        }
    }
    for step in 0..=segments {
        let along = start + (end - start) * step as f32 / segments as f32;
        columns.push(profile(
            field,
            edge(bounds, side, along).0,
            arc_normal(side, 0),
        ));
    }
    if end == length {
        let anchor = edge(bounds, side, length).0;
        for step in 1..=CORNER_STEPS {
            columns.push(profile(field, anchor, arc_normal(side, step)));
        }
    }
    columns
}

pub(super) fn cliff(field: &TerrainField, side: u32, chunk: i32) -> Mesh {
    let columns = columns(field, side, chunk);
    let mut geometry = Geometry::default();
    let mut tags = Vec::new();
    for (step, pair) in columns.windows(2).enumerate() {
        let [previous, next] = [pair[0], pair[1]];
        for layer in 0..LAYERS - 1 {
            let [a, b, c, d] = [
                previous[layer],
                next[layer],
                previous[layer + 1],
                next[layer + 1],
            ];
            let shade = 0.86
                + random(
                    (chunk as u32).wrapping_mul(773)
                        ^ step as u32
                        ^ (layer as u32).wrapping_mul(337),
                ) * 0.18;
            let color = if layer == 0 {
                Vec3::ONE
            } else if layer >= 4 {
                Vec3::new(0.78, 0.73, 0.62)
            } else {
                Vec3::new(0.43, 0.41, 0.40) * shade
            };
            let color = Color::srgb(color.x, color.y, color.z).to_linear();
            for points in [[a, c, b], [b, c, d]] {
                // The top ring collapses to the core corner during arc steps.
                // Emit its fan once; never normalize a zero-area triangle.
                if (points[1] - points[0])
                    .cross(points[2] - points[0])
                    .length_squared()
                    < 0.000001
                {
                    continue;
                }
                geometry.triangle(points, color, false);
                tags.extend([if layer == 0 { 0.0 } else { 1.0 }; 3]);
            }
        }
    }
    let mut mesh = geometry.rigid_mesh();
    let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR)
    else {
        unreachable!()
    };
    // Alpha is a surface tag, not transparency: the coast material is opaque.
    // Duplicate per-triangle vertices keep grass/cliff tags from interpolating.
    for (color, tag) in colors.iter_mut().zip(tags) {
        color[3] = tag;
    }
    mesh
}

pub(super) fn boundary_collider(bounds: Vec4) -> Mesh {
    let mut mesh: Option<Mesh> = None;
    for side in 0..4 {
        let length = side_length(bounds, side);
        let (center, outward) = edge(bounds, side, length * 0.5);
        let center = center + outward * 5.0;
        let size = if side % 2 == 0 {
            Vec3::new(length, 50.0, 10.0)
        } else {
            Vec3::new(10.0, 50.0, length)
        };
        let wall = Cuboid::from_size(size)
            .mesh()
            .build()
            .transformed_by(Transform::from_xyz(center.x, 25.0, -center.y));
        if let Some(mesh) = &mut mesh {
            mesh.merge(&wall).unwrap();
        } else {
            mesh = Some(wall);
        }
    }
    mesh.unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coast_profile::coast_distance;

    fn positions(mesh: &Mesh) -> &[[f32; 3]] {
        match mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() {
            VertexAttributeValues::Float32x3(points) => points,
            _ => panic!("cliff positions must be Float32x3"),
        }
    }

    fn outside(point: Vec3, bounds: Vec4) -> bool {
        point.x <= bounds.x || point.x >= bounds.z || -point.z <= bounds.y || -point.z >= bounds.w
    }

    #[test]
    fn rounded_cliffs_have_finite_outward_faces_and_only_the_apron_is_tagged_as_grass() {
        let field = TerrainField::from_parcels(&[[0, 0]], true).unwrap();
        let bounds = bounds(&field);
        let chunks = near_chunks(bounds, Vec3::ZERO);
        assert_eq!(chunks.len(), 8);
        for (side, chunk) in chunks {
            let mesh = cliff(&field, side, chunk);
            assert!(mesh.count_vertices() <= 900);
            assert_eq!(mesh.attributes().count(), 3);
            let Some(VertexAttributeValues::Float32x3(normals)) =
                mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
            else {
                panic!("cliff normals missing")
            };
            let Some(VertexAttributeValues::Float32x4(colors)) =
                mesh.attribute(Mesh::ATTRIBUTE_COLOR)
            else {
                panic!("cliff surface tags missing")
            };
            let points = positions(&mesh);
            assert!(points.iter().all(|p| Vec3::from_array(*p).is_finite()));
            assert!(points.iter().any(|p| p[1] == 0.0));
            assert!(points.iter().any(|p| p[1] < SEA_LEVEL));
            let mut grass = 0;
            let mut rock = 0;
            for (index, triangle) in points.chunks_exact(3).enumerate() {
                let p = [0, 1, 2].map(|i| Vec3::from_array(triangle[i]));
                let n = Vec3::from_array(normals[index * 3]);
                assert!(n.is_finite() && (n.length() - 1.0).abs() < 0.001);
                assert!((p[1] - p[0]).cross(p[2] - p[0]).dot(n) > 0.0001);
                // Check interiors, not only vertices: no cap or arc fan may
                // cut across the reserved playable rectangle.
                for sample in [
                    p[0],
                    p[1],
                    p[2],
                    (p[0] + p[1] + p[2]) / 3.0,
                    (p[0] + p[1]) * 0.5,
                    (p[1] + p[2]) * 0.5,
                    (p[2] + p[0]) * 0.5,
                ] {
                    assert!(outside(sample, bounds));
                }
                let tags = &colors[index * 3..index * 3 + 3];
                assert!(tags.iter().all(|color| color[3] == tags[0][3]));
                if tags[0][3] == 0.0 {
                    grass += 1;
                    assert!(n.y > 0.99);
                    assert!(p.iter().all(|point| point.y >= -0.151));
                } else {
                    rock += 1;
                    assert_eq!(tags[0][3], 1.0);
                    let center = (p[0] + p[1] + p[2]) / 3.0;
                    let unity = Vec2::new(center.x, -center.z);
                    let outward = unity - unity.clamp(bounds.xy(), bounds.zw());
                    assert!(Vec2::new(n.x, -n.z).dot(outward) > 0.0);
                }
            }
            assert!(grass > 0 && rock > 0);
        }
        let huge = Vec4::new(-16000.0, -16000.0, 16000.0, 16000.0);
        assert!(near_chunks(huge, Vec3::ZERO).is_empty());
        assert!(near_chunks(huge, Vec3::new(16000.0, 0.0, -16000.0)).len() <= 28);
    }

    #[test]
    fn arc_and_side_waterline_vertices_follow_the_shared_round_distance_field() {
        let field = TerrainField::from_parcels(&[[0, 0], [20, 20]], true).unwrap();
        let bounds = bounds(&field);
        for side in 0..4 {
            let length = side_length(bounds, side);
            let chunks = (length / CHUNK).ceil() as i32;
            for chunk in 0..chunks {
                let columns = columns(&field, side, chunk);
                for column in &columns {
                    let waterline = column[6];
                    assert!(column[5].y > SEA_LEVEL + 1.0);
                    assert_eq!(waterline.y, SEA_LEVEL);
                    assert!(
                        (coast_distance(Vec2::new(waterline.x, -waterline.z), bounds)).abs()
                            < 0.001
                    );
                    assert!((8.699..=12.301).contains(&(waterline - column[5]).xz().length()));
                    assert!(column.windows(2).all(|pair| pair[0].y > pair[1].y));
                }
                // Arc chords and four-metre side samples approximate the same
                // smooth field within 12 cm, below the surf feather width.
                for pair in columns.windows(2) {
                    let midpoint = (pair[0][6] + pair[1][6]) * 0.5;
                    assert!(
                        coast_distance(Vec2::new(midpoint.x, -midpoint.z), bounds).abs() < 0.12
                    );
                }
            }
        }
    }

    #[test]
    fn varying_crest_and_beach_remain_entirely_outside_the_unchanged_core() {
        let field = TerrainField::from_parcels(&[[-30, -20], [40, 35]], true).unwrap();
        let bounds = bounds(&field);
        let mut low = Vec2::splat(f32::INFINITY);
        let mut high = Vec2::splat(f32::NEG_INFINITY);
        for side in 0..4 {
            let length = side_length(bounds, side);
            for along in (0..=length as i32).step_by(4) {
                let (edge, _) = edge(bounds, side, along as f32);
                let column = profile(&field, edge, arc_normal(side, 0));
                assert_eq!(Vec2::new(column[0].x, -column[0].z), edge);
                assert!(column.windows(2).all(|pair| pair[0].y > pair[1].y));
                assert!((column[0].y - column[1].y - 0.15).abs() < 0.0001);
                for point in column {
                    assert!(outside(point, bounds));
                    assert!((point.xz() - column[0].xz()).length() <= 41.001);
                }
                let radii = Vec2::new(
                    (column[1] - column[0]).xz().length(),
                    (column[6] - column[0]).xz().length(),
                );
                low = low.min(radii);
                high = high.max(radii);
            }
        }
        assert!((high - low).min_element() > 8.0);
    }

    #[test]
    fn neighboring_chunks_and_corner_halves_share_exact_vertices_on_every_ring() {
        let field = TerrainField::from_parcels(&[[-30, -20], [40, 35]], true).unwrap();
        let bounds = bounds(&field);
        for side in 0..4 {
            let chunks = (side_length(bounds, side) / CHUNK).ceil() as i32;
            for chunk in 0..chunks {
                let (next_side, next_chunk) = if chunk + 1 == chunks {
                    ((side + 1) % 4, 0)
                } else {
                    (side, chunk + 1)
                };
                let a = columns(&field, side, chunk);
                let b = columns(&field, next_side, next_chunk);
                assert_eq!(a.last(), b.first());
                let left = cliff(&field, side, chunk);
                let right = cliff(&field, next_side, next_chunk);
                assert!(left.count_vertices() <= 900);
                assert_eq!(left.attributes().count(), 3);
                for point in a.last().unwrap() {
                    assert!(positions(&left).contains(&point.to_array()));
                    assert!(positions(&right).contains(&point.to_array()));
                }
                if chunk > 0 && chunk + 1 < chunks {
                    assert_eq!(left.count_vertices(), 672);
                }
            }
        }
    }

    #[test]
    fn even_two_corner_halves_in_a_single_small_chunk_stay_under_900_vertices() {
        let mut field = TerrainField::from_parcels(&[[0, 0]], true).unwrap();
        field.min_parcel = [0, 0];
        field.max_parcel = [3, 3];
        for side in 0..4 {
            let mesh = cliff(&field, side, 0);
            assert_eq!(mesh.count_vertices(), 894);
            assert!(positions(&mesh)
                .iter()
                .all(|p| Vec3::from_array(*p).is_finite()));
        }
    }
}
