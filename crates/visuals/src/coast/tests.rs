use bevy::render::mesh::VertexAttributeValues;

use super::*;

fn bounds(min: IVec2, max: IVec2) -> Vec4 {
    let (min, max) = ((min * 16).as_vec2(), ((max + 1) * 16).as_vec2());
    Vec4::new(min.x, min.y, max.x, max.y)
}

fn outside(point: Vec3, bounds: Vec4) -> f32 {
    let unity = Vec2::new(point.x, -point.z);
    (bounds.xy() - unity)
        .max(unity - bounds.zw())
        .max(Vec2::ZERO)
        .length()
}

/// coast_profile.wgsl's `coast_distance`.
fn coast_distance(point: Vec3, bounds: Vec4) -> f32 {
    let unity = Vec2::new(point.x, -point.z);
    outside(point, bounds) - coast_profile(unity.clamp(bounds.xy(), bounds.zw())).y
}

fn positions(mesh: &Mesh) -> &[[f32; 3]] {
    match mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() {
        VertexAttributeValues::Float32x3(points) => points,
        _ => panic!("cliff positions must be Float32x3"),
    }
}

fn chunks(bounds: Vec4, side: u32) -> i32 {
    (side_length(bounds, side) / CHUNK).ceil() as i32
}

#[test]
fn cliffs_face_outward_and_stay_outside_the_ground() {
    let bounds = bounds(IVec2::new(-2, -2), IVec2::new(2, 2));
    let near = near_chunks(bounds, Vec3::ZERO, CLIFF_RANGE);
    assert_eq!(near.len(), 8);
    for (side, chunk) in near {
        let mesh = cliff_mesh(bounds, side, chunk);
        let Some(VertexAttributeValues::Float32x3(normals)) =
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!("cliff normals missing")
        };
        let points = positions(&mesh);
        assert!(!points.is_empty());
        for (index, triangle) in points.chunks_exact(3).enumerate() {
            let p = [0, 1, 2].map(|i| Vec3::from_array(triangle[i]));
            let n = Vec3::from_array(normals[index * 3]);
            if p.iter().all(|point| point.y == UNDERGROUND) {
                // the hidden lid, from the bounds to the crest, facing up
                assert_eq!(n, Vec3::Y);
                assert!((p[1] - p[0]).cross(p[2] - p[0]).y > 0.0);
                continue;
            }
            assert!(n.is_finite() && (n.length() - 1.0).abs() < 0.001);
            // counter-clockwise seen from the side the normal faces
            assert!((p[1] - p[0]).cross(p[2] - p[0]).dot(n) > 0.0001);
            let centre = (p[0] + p[1] + p[2]) / 3.0;
            let unity = Vec2::new(centre.x, -centre.z);
            let outward = unity - unity.clamp(bounds.xy(), bounds.zw());
            assert!(Vec2::new(n.x, -n.z).dot(outward) > 0.0 || n.y > 0.0);
            // the crest is the nearest the cliff comes to the bounds
            for point in p {
                assert!(outside(point, bounds) > 1.9);
                assert!(point.y <= GROUND_LEVEL);
            }
        }
    }
    let huge = Vec4::new(-16000.0, -16000.0, 16000.0, 16000.0);
    assert!(near_chunks(huge, Vec3::ZERO, CLIFF_RANGE).is_empty());
    assert!(near_chunks(huge, Vec3::new(16000.0, 0.0, -16000.0), CLIFF_RANGE).len() <= 28);
}

#[test]
fn profiles_descend_from_the_ground_to_below_the_sea_on_the_shared_waterline() {
    let bounds = bounds(IVec2::new(-30, -20), IVec2::new(40, 35));
    for side in 0..4 {
        for chunk in 0..chunks(bounds, side) {
            let columns = columns(bounds, side, chunk);
            for column in &columns {
                assert_eq!(column[0].y, GROUND_LEVEL);
                assert!(column.windows(2).all(|pair| pair[0].y > pair[1].y));
                let waterline = column[5];
                assert_eq!(waterline.y, SEA_LEVEL);
                assert!(coast_distance(waterline, bounds).abs() < 0.001);
                assert!(column.iter().all(|point| outside(*point, bounds) <= 41.001));
            }
            // 1 m chords and corner facets stay within 12 cm of the smooth waterline
            for pair in columns.windows(2) {
                let midpoint = (pair[0][5] + pair[1][5]) * 0.5;
                assert!(coast_distance(midpoint, bounds).abs() < 0.12);
            }
        }
    }
}

#[test]
fn neighbouring_chunks_and_corner_halves_share_their_end_columns() {
    let bounds = bounds(IVec2::new(-30, -20), IVec2::new(40, 35));
    for side in 0..4 {
        let count = chunks(bounds, side);
        for chunk in 0..count {
            let (next_side, next_chunk) = if chunk + 1 == count {
                ((side + 1) % 4, 0)
            } else {
                (side, chunk + 1)
            };
            let a = columns(bounds, side, chunk);
            let b = columns(bounds, next_side, next_chunk);
            assert_eq!(a.last(), b.first());
        }
    }
}

#[test]
fn a_single_parcel_world_gets_a_closed_coast() {
    let bounds = bounds(IVec2::new(-2, -2), IVec2::new(2, 2));
    for side in 0..4 {
        assert_eq!(chunks(bounds, side), 2);
        let last = columns(bounds, side, 1).last().copied();
        let first = columns(bounds, (side + 1) % 4, 0).first().copied();
        assert_eq!(last, first);
    }
}

#[test]
fn walls_stand_outside_the_crest_with_their_inner_face_on_it() {
    let bounds = bounds(IVec2::new(-3, -1), IVec2::new(4, 6));
    for side in 0..4 {
        for chunk in 0..chunks(bounds, side) {
            let crest: Vec<_> = wall_columns(bounds, side, chunk)
                .iter()
                .map(|column| column[0])
                .collect();
            let walls = crest_walls(bounds, side, chunk);
            assert_eq!(walls.len(), crest.len() - 1);
            for ((centre, half, rotation), pair) in walls.iter().zip(crest.windows(2)) {
                assert_eq!(half.y * 2.0, WALL_HEIGHT);
                assert_eq!(half.z * 2.0, WALL_THICKNESS);
                assert!(centre.y - half.y == 0.0);
                // both crest ends lie on the box's inner face, at its ends
                for (point, end) in pair.iter().zip([-1.0, 1.0]) {
                    let local = rotation.inverse() * (point.with_y(centre.y) - *centre);
                    assert!((local.x - end * half.x).abs() < 0.001, "{local}");
                    assert!((local.z.abs() - half.z).abs() < 0.001, "{local}");
                }
                // and the rest of the wall is further out than the crest
                assert!(
                    outside(*centre, bounds)
                        > outside(pair[0], bounds).min(outside(pair[1], bounds))
                );
            }
        }
    }
}

#[test]
fn shader_profile_matches_the_cpu_profile() {
    let shader = include_str!("../coast_profile.wgsl");
    for expression in [
        "sin(dot(edge, vec2(0.013, 0.019)) + 0.7)",
        "sin(dot(edge, vec2(0.041, -0.027)) + 2.2)",
        "sin(dot(edge, vec2(0.007, -0.011)) - 0.4)",
        "8.0 + 4.0 * a + 2.0 * b",
        "crest + 18.0 + 2.0 * c + b",
    ] {
        assert!(shader.contains(expression), "profile drift: {expression}");
    }
    assert_eq!(shader.matches("sin(").count(), 3);
}

#[test]
fn ripples_tile_and_are_deterministic() {
    for cells in [4, 5, 7, 13, 29, 61] {
        for i in 0..=128 {
            let t = i as f32 / 128.0;
            assert_eq!(
                ripples::noise(Vec2::new(0.0, t), cells, 37),
                ripples::noise(Vec2::new(1.0, t), cells, 37)
            );
            assert_eq!(
                ripples::noise(Vec2::new(t, 0.0), cells, 37),
                ripples::noise(Vec2::new(t, 1.0), cells, 37)
            );
        }
    }
    for i in 0..=128 {
        let p = Vec2::new(-0.25, i as f32 / 128.0);
        for offset in [Vec2::X, Vec2::Y] {
            assert!((ripples::height_field(p) - ripples::height_field(p + offset)).abs() < 0.00005);
        }
    }
    let image = ripples::image();
    let bytes = image.data.as_ref().unwrap();
    // every mip of a 256 square RG8 texture
    assert_eq!(bytes.len(), 174762);
    assert_eq!(image.texture_descriptor.mip_level_count, 9);
    assert_eq!(bytes, ripples::image().data.as_ref().unwrap());
    let top = &bytes[..ripples::SIZE * ripples::SIZE * 2];
    assert!(top.iter().any(|&v| v < 100) && top.iter().any(|&v| v > 155));
}
