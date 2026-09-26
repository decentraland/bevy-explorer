use bevy::render::mesh::VertexAttributeValues;
use common::terrain::Occupancy;
use scene_runner::update_world::mesh_collider::GROUND_COLLISION_MASK;

use super::*;

fn ground_at(data: &mut SceneColliderData, x: f32, z: f32) -> f32 {
    let hit = data
        .cast_ray_nearest(
            Vec3::new(x, 50.0, z),
            Vec3::NEG_Y,
            100.0,
            GROUND_COLLISION_MASK,
            false,
            false,
            None,
        )
        .expect("terrain hit");
    50.0 - hit.toi
}

/// unity-explorer's `CollideTerrainSystem.CreateIndexBuffer` splits cells along
/// (x0, z0)-(x1, z1) in Unity space. Raising the (x1, z1) corner of one cell gives min(u, v)
/// with that split and max(0, u + v - 1) with the other.
#[test]
fn heightfield_split_matches_unity() {
    let mut data = SceneColliderData::default();
    // one cell over bevy x 0..1, z -1..0 (unity z 0..1); rows along +bevy z, so row 0 is unity z 1
    data.set_ground_heightfield(
        &collider_id(),
        2,
        2,
        &[0.0, 1.0, 0.0, 0.0],
        Vec2::ONE,
        Vec3::new(0.5, 0.0, -0.5),
    );
    for (u, v) in [(0.25, 0.75), (0.75, 0.25), (0.4, 0.5), (0.9, 0.8)] {
        let height = ground_at(&mut data, u, -v);
        assert!((height - f32::min(u, v)).abs() < 1e-4, "{u},{v}: {height}");
    }
}

/// The render grids use the same split, so what you see is what you stand on.
#[test]
fn mesh_split_matches_unity() {
    for mesh in [
        parcel_mesh(16),
        ground_mesh(|offset| 16 >> (offset.x.abs() % 3), 2),
    ] {
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions");
        };
        let Some(Indices::U32(indices)) = mesh.indices() else {
            panic!("indices");
        };
        for triangle in indices.chunks_exact(3) {
            let corners: [Vec3; 3] =
                std::array::from_fn(|i| Vec3::from(positions[triangle[i] as usize]));
            // front faces point up
            let normal = (corners[1] - corners[0]).cross(corners[2] - corners[0]);
            assert!(normal.y > 0.0);
            let diagonals = (0..3)
                .map(|edge| corners[(edge + 1) % 3] - corners[edge])
                .filter(|delta| delta.x != 0.0 && delta.z != 0.0)
                .collect::<Vec<_>>();
            assert_eq!(diagonals.len(), 1);
            // unity z = -bevy z
            assert!(diagonals[0].x * -diagonals[0].z > 0.0);
        }
    }
}

/// A realm with one occupied parcel at the origin and everything else empty, out to `extent`.
fn surface(extent: i32) -> (TerrainTargets, TerrainSurface) {
    let mut targets = TerrainTargets::default();
    targets.reset(false);
    targets.mark(IVec2::ZERO, true);
    // occupied bounds come from a single parcel; widen them with occupied corners
    let corners = [IVec2::splat(-extent), IVec2::splat(extent)];
    let occupancy = move |parcel: IVec2| {
        if parcel == IVec2::ZERO || corners.contains(&parcel) {
            Occupancy::Occupied
        } else {
            Occupancy::Empty
        }
    };
    let change = targets.resolve(occupancy, [IVec2::ZERO, corners[0], corners[1]].into_iter());
    let mut surface = TerrainSurface::default();
    surface.apply(&targets, change, true);
    (targets, surface)
}

#[test]
fn collider_follows_the_height_function_at_grid_points() {
    let (_, surface) = surface(20);
    let centre = IVec2::new(40, -40);
    let heights = collider_heights(&surface, centre);
    assert!(heights.iter().any(|height| *height > 3.0));
    let mut data = SceneColliderData::default();
    let side = (COLLIDER_SIZE + 1) as usize;
    data.set_ground_heightfield(
        &collider_id(),
        side,
        side,
        &heights,
        Vec2::splat(COLLIDER_SIZE as f32),
        Vec3::new(centre.x as f32, 0.0, centre.y as f32),
    );
    for (dx, dz) in [(-31, -31), (-7, 3), (0, 0), (5, 12), (30, -2), (31, 31)] {
        let (x, z) = ((centre.x + dx) as f32, (centre.y + dz) as f32);
        let expected = surface.height(x, -z);
        let actual = ground_at(&mut data, x, z);
        assert!(
            (actual - expected).abs() < 1e-3,
            "{x},{z}: {actual} vs {expected}"
        );
    }
}

#[test]
fn easing_rises_one_step_per_second_and_settles_on_the_targets() {
    let (targets, mut surface) = surface(12);
    // a new realm snaps; clear it to watch a reveal ease in
    let mut revealed = TerrainSurface {
        epoch: surface.epoch,
        enabled: true,
        ..Default::default()
    };
    revealed.apply(&targets, Some(TerrainChange::Full), true);
    let parcel = IVec2::new(6, 6);
    let x = parcel.x as f32 * 16.0 + 8.0;
    let z = parcel.y as f32 * 16.0 + 8.0;
    let final_height = surface.height(x, z);
    assert!(final_height > 5.0);
    assert_eq!(revealed.height(x, z), 0.0);

    let mut previous = 0.0;
    for _ in 0..(f32::from(MAX_STEPS) / 0.1) as usize + 2 {
        revealed.ease(0.1);
        let height = revealed.height(x, z);
        assert!(height >= previous - 1e-4);
        // one step is 1.8 m; the noise weight (up to ~4.4 m) also ramps in over the first
        // 0.4 steps
        assert!(
            height - previous <= 0.1 * 1.8 + 0.25 * 4.5,
            "{previous} -> {height}"
        );
        previous = height;
    }
    assert!((previous - final_height).abs() < 1e-4);
    assert!(revealed.active.is_empty());
    // a snapped surface has nothing to ease
    surface.changes = SurfaceChanges::default();
    surface.ease(1.0);
    assert!(surface.changes.eased.is_none());
}

#[test]
fn scene_casts_hit_the_heightfield() {
    use bevy::math::DVec3;
    use dcl_component::proto_components::sdk::components::ColliderLayer;
    let mut data = SceneColliderData::default();
    let side = (COLLIDER_SIZE + 1) as usize;
    data.set_ground_heightfield(
        &collider_id(),
        side,
        side,
        &vec![3.0; side * side],
        Vec2::splat(COLLIDER_SIZE as f32),
        Vec3::new(10.0, 0.0, -10.0),
    );
    // the movement scene's ground casts use CL_PHYSICS alone
    let mask = ColliderLayer::ClPhysics as u32;
    for skip_inside in [false, true] {
        let ray = data
            .cast_ray_nearest(
                Vec3::new(12.0, 10.0, -12.0),
                Vec3::NEG_Y,
                20.0,
                mask,
                skip_inside,
                false,
                None,
            )
            .map(|hit| hit.toi);
        assert_eq!(ray, Some(7.0), "ray, skip_inside {skip_inside}");
        let avatar = data
            .cast_avatar_nearest(
                DVec3::new(12.0, 10.0, -12.0),
                DVec3::NEG_Y,
                20.0,
                mask,
                skip_inside,
                false,
                Default::default(),
                0.0,
            )
            .map(|hit| hit.toi);
        assert!(
            avatar.is_some_and(|toi| (toi - 7.0).abs() < 1e-3),
            "avatar, skip_inside {skip_inside}: {avatar:?}"
        );
        let all = data.cast_avatar_all(
            DVec3::new(12.0, 10.0, -12.0),
            DVec3::NEG_Y,
            20.0,
            mask,
            skip_inside,
            false,
            0.0,
        );
        assert_eq!(all.len(), 1, "avatar all, skip_inside {skip_inside}");
    }
}
