use super::*;

#[test]
fn grove_field_is_stable_clustered_and_does_not_increase_the_population_budget() {
    let mut previous_count = 0;
    let mut grove_count = 0;
    let mut clear = 0;
    let mut dense = 0;
    let mut adjacent_difference = 0.0;
    for x in -64..64 {
        for y in -64..64 {
            let parcel = IVec2::new(x, y);
            let density = grove_density(parcel);
            assert_eq!(density, grove_density(parcel));
            assert!((0.0..=1.0).contains(&density));
            clear += usize::from(density < 0.1);
            dense += usize::from(density > 0.9);
            adjacent_difference += (density - grove_density(parcel + IVec2::X)).abs();
            let seed =
                (x as u32).wrapping_mul(73_856_093) ^ (y as u32).wrapping_mul(19_349_663) ^ 0x67ae;
            previous_count += usize::from(random(seed) <= 0.68);
            grove_count += usize::from(random(seed) <= 0.16 + 0.78 * density);
        }
    }
    assert!(
        clear > 1000 && dense > 1000,
        "retain both groves and clearings"
    );
    assert!(adjacent_difference / (128.0 * 128.0) < 0.22);
    assert!(grove_count < previous_count);
    assert!(
        grove_count > previous_count / 2,
        "do not empty the landscape"
    );
}

fn gentle(x: f32, z: f32) -> f32 {
    0.05 * x + 0.03 * z
}

#[test]
fn placement_is_stable_terrain_anchored_and_clear_of_non_empty_land() {
    let empty = |parcel: IVec2| parcel != IVec2::ZERO;
    let trees: Vec<_> = (-6..=6)
        .flat_map(|x| (-6..=6).map(move |y| IVec2::new(x, y)))
        .filter_map(|parcel| Some((parcel, placement(parcel, empty, gentle)?)))
        .collect();
    assert!(trees.len() > 8);
    assert!(trees.iter().any(|(parcel, _)| parcel.x < 0 && parcel.y < 0));
    for (parcel, tree) in &trees {
        assert_eq!(Some(*tree), placement(*parcel, empty, gentle));
        let p = tree.transform.translation;
        assert!((p.y + SINK - gentle(p.x, -p.z)).abs() < 0.0001);
        assert_eq!(vec3_to_parcel(p), *parcel);
        let center = parcel.as_vec2() * 16.0 + 8.0;
        assert!(Vec2::new(p.x, -p.z).distance(center) >= 1.99);
        assert!(tree.kind < KINDS);
        // the crown stays over empty parcels
        let radius = CROWN_RADIUS * tree.transform.scale.x;
        for corner in [Vec2::splat(-radius), Vec2::splat(radius)] {
            let reach = Vec2::new(p.x, -p.z) + corner;
            assert!(empty((reach / 16.0).floor().as_ivec2()));
        }
    }
}

#[test]
fn steep_ground_has_no_trees() {
    let steep = |x: f32, _: f32| x;
    assert!((-6..=6)
        .flat_map(|x| (-6..=6).map(move |y| IVec2::new(x, y)))
        .all(|parcel| placement(parcel, |_| true, steep).is_none()));
}
