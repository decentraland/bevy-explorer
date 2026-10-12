use super::*;

fn gentle(x: f32, z: f32) -> f32 {
    0.05 * x + 0.03 * z
}

#[test]
fn props_are_stable_terrain_anchored_and_inside_their_empty_parcel() {
    let empty = |parcel: IVec2| parcel != IVec2::ZERO;
    assert!(placements(IVec2::ZERO, empty, gentle).is_empty());
    let mut rocks = 0;
    let mut bushes = 0;
    for x in -8..=8 {
        for y in -8..=8 {
            let parcel = IVec2::new(x, y);
            let props = placements(parcel, empty, gentle);
            assert_eq!(props, placements(parcel, empty, gentle));
            for prop in props {
                let p = prop.transform.translation;
                assert_eq!(prop.key.parcel, parcel);
                assert_eq!(vec3_to_parcel(p), parcel);
                assert!((p.y + SINK - gentle(p.x, -p.z)).abs() < 0.0001);
                let local = Vec2::new(p.x, -p.z) - parcel.as_vec2() * 16.0;
                assert!(
                    local.cmpge(Vec2::splat(2.49)).all() && local.cmple(Vec2::splat(13.51)).all()
                );
                assert!(local.distance(Vec2::splat(8.0)) >= 2.0);
                rocks += usize::from(prop.kind < ROCKS);
                bushes += usize::from(prop.kind >= ROCKS);
            }
        }
    }
    assert!(rocks > 50 && bushes > 100, "rocks {rocks} bushes {bushes}");
}

#[test]
fn steep_ground_has_no_props() {
    let steep = |x: f32, _: f32| x * 1.5;
    assert!((-6..=6)
        .flat_map(|x| (-6..=6).map(move |y| IVec2::new(x, y)))
        .all(|parcel| placements(parcel, |_| true, steep).is_empty()));
}
