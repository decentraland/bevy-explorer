use super::*;
use std::sync::Arc;

fn occupied() -> PointerResult {
    PointerResult::Exists {
        realm: "test".into(),
        hash: "scene".into(),
        urn: None,
    }
}

fn fixture() -> (TerrainField, ScenePointers) {
    let field = TerrainField::from_parcels(&[[0, 0]], true).unwrap();
    let mut pointers = ScenePointers::default();
    pointers.set_realm(IVec2::splat(-20), IVec2::splat(20));
    for y in -10..=10 {
        for x in -10..=10 {
            pointers.insert(IVec2::new(x, y), PointerResult::Nothing);
        }
    }
    pointers.insert(IVec2::ZERO, occupied());
    (field, pointers)
}

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

#[test]
fn placement_is_stable_terrain_anchored_and_clear_of_occupied_or_unknown_land() {
    let (field, mut pointers) = fixture();
    let trees: Vec<_> = (-6..=6)
        .flat_map(|x| (-6..=6).map(move |y| IVec2::new(x, y)))
        .filter_map(|parcel| placement(parcel, &field, &pointers))
        .collect();
    assert!(trees.len() > 8);
    assert!(trees
        .iter()
        .any(|tree| tree.parcel.x < 0 && tree.parcel.y < 0));
    for tree in &trees {
        assert_eq!(Some(*tree), placement(tree.parcel, &field, &pointers));
        let p = tree.transform.translation;
        assert!((p.y + 0.06 - field.height(p.x, -p.z)).abs() < 0.0001);
        assert_eq!(vec3_to_parcel(p), tree.parcel);
        let center = Vec2::new(
            tree.parcel.x as f32 * 16.0 + 8.0,
            tree.parcel.y as f32 * 16.0 + 8.0,
        );
        assert!(Vec2::new(p.x, -p.z).distance(center) >= 1.99);
        assert!(tree.kind < KINDS);
        assert!(tree.transform.scale.x >= 0.8 && tree.transform.scale.x <= 1.2);
        let radius = CROWN_RADIUS * tree.transform.scale.x + 0.4;
        for direction in [Vec2::X, Vec2::NEG_X, Vec2::Y, Vec2::NEG_Y] {
            let edge = Vec2::new(p.x, -p.z) + direction * radius;
            assert!(empty(
                IVec2::new(
                    (edge.x / 16.0).floor() as i32,
                    (edge.y / 16.0).floor() as i32
                ),
                &field,
                &pointers
            ));
        }
    }
    assert!(placement(IVec2::ZERO, &field, &pointers).is_none());
    assert!(placement(IVec2::splat(500), &field, &pointers).is_none());
    let parcel = trees[0].parcel;
    pointers.insert(parcel, occupied());
    assert!(placement(parcel, &field, &pointers).is_none());
    assert!(placement(parcel, &field, &ScenePointers::default()).is_none());
}

#[test]
fn trees_follow_hills_but_reject_steep_ground() {
    let parcels: Vec<_> = (0..20).flat_map(|x| (0..20).map(move |z| [x, z])).collect();
    let field = TerrainField::from_parcels(&parcels, true).unwrap();
    let mut pointers = ScenePointers::default();
    pointers.set_realm(IVec2::ZERO, IVec2::splat(19));
    let mut hills = 0;
    for x in -10..0 {
        for z in -10..30 {
            let parcel = IVec2::new(x, z);
            if let Some(tree) = placement(parcel, &field, &pointers) {
                let p = tree.transform.translation;
                assert!((p.y + 0.06 - field.height(p.x, -p.z)).abs() < 0.0001);
                if p.y > 0.5 {
                    hills += 1;
                }
                let mut steep = field.clone();
                steep.max_steps = 10_000;
                let samples = [
                    steep.height(p.x - 1.5, -p.z),
                    steep.height(p.x + 1.5, -p.z),
                    steep.height(p.x, -p.z - 1.5),
                    steep.height(p.x, -p.z + 1.5),
                ];
                if samples
                    .iter()
                    .any(|h| (h - steep.height(p.x, -p.z)).abs() > 0.85)
                {
                    assert!(placement(parcel, &steep, &pointers).is_none());
                }
            }
        }
    }
    assert!(hills > 0, "fixture must exercise non-flat terrain");
}

#[test]
fn lod_hysteresis_keeps_camera_jitter_from_rebuilding_meshes() {
    assert_eq!(lod(50.0, 0, TreeMode::Auto), 0);
    assert_eq!(lod(50.0, 1, TreeMode::Auto), 1);
    assert_eq!(lod(96.0, 1, TreeMode::Auto), 1);
    assert_eq!(lod(96.0, 2, TreeMode::Auto), 2);
    assert_eq!(lod(110.0, 1, TreeMode::Auto), 2);
    assert_eq!(lod(20.0, 2, TreeMode::Auto), 0);
    assert_eq!(lod(150.0, 2, TreeMode::High), 0);
    assert_eq!(lod(2.0, 0, TreeMode::Low), 2);
}

fn app() -> App {
    let (field, pointers) = fixture();
    let mut app = App::new();
    app.init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<TreeMaterial>>()
        .init_resource::<Assets<Image>>()
        .init_resource::<TreeMode>()
        .init_resource::<TreeState>()
        .insert_resource(GroundVisibility(true))
        .insert_resource(LandscapeTerrain {
            generation: 1,
            field: Some(Arc::new(field)),
            ..default()
        })
        .insert_resource(pointers)
        .add_systems(Startup, setup)
        .add_systems(Update, refresh);
    app.world_mut().spawn((
        PrimaryUser::default(),
        GlobalTransform::from_translation(Vec3::new(-24.0, 1.0, 24.0)),
    ));
    app
}

fn trees(app: &mut App) -> Vec<(Entity, IVec2, Transform)> {
    let world = app.world_mut();
    let mut query = world.query::<(Entity, &LandscapeTree, &Transform)>();
    let mut trees: Vec<_> = query
        .iter(world)
        .map(|(e, tree, t)| (e, tree.parcel, *t))
        .collect();
    trees.sort_by_key(|(_, parcel, _)| (parcel.x, parcel.y));
    trees
}

fn collider_count(app: &mut App) -> usize {
    let world = app.world_mut();
    world
        .query_filtered::<Entity, With<TreeColliders>>()
        .iter(world)
        .count()
}

#[test]
fn off_restore_realm_reset_and_terrain_opt_out_remove_meshes_and_collisions() {
    let mut app = app();
    app.update();
    let initial = trees(&mut app);
    assert!(!initial.is_empty());
    assert_eq!(collider_count(&mut app), 1);
    let placement: Vec<_> = initial.iter().map(|(_, p, t)| (*p, *t)).collect();
    *app.world_mut().resource_mut::<TreeMode>() = TreeMode::Off;
    app.update();
    assert!(trees(&mut app).is_empty());
    assert_eq!(collider_count(&mut app), 0);
    assert!(initial
        .iter()
        .all(|(entity, _, _)| app.world().get_entity(*entity).is_err()));
    *app.world_mut().resource_mut::<TreeMode>() = TreeMode::Auto;
    app.update();
    assert_eq!(
        trees(&mut app)
            .iter()
            .map(|(_, p, t)| (*p, *t))
            .collect::<Vec<_>>(),
        placement
    );
    assert_eq!(app.world().resource::<Assets<Mesh>>().len(), KINDS * 3);
    app.world_mut().resource_mut::<GroundVisibility>().0 = false;
    app.update();
    assert!(trees(&mut app).is_empty());
    assert_eq!(collider_count(&mut app), 0);
    app.world_mut().resource_mut::<GroundVisibility>().0 = true;
    app.update();
    assert!(!trees(&mut app).is_empty());
    let mut terrain = app.world_mut().resource_mut::<LandscapeTerrain>();
    terrain.generation += 1;
    terrain.field = None;
    app.update();
    assert!(trees(&mut app).is_empty());
    assert_eq!(collider_count(&mut app), 0);
}

#[test]
fn newly_occupied_parcels_remove_their_tree_and_far_teleports_clear_all_colliders() {
    let mut app = app();
    app.update();
    let initial = trees(&mut app);
    let (entity, parcel, _) = initial[0];
    // The newly occupied rectangle must evict both its own trunk and any
    // neighboring crown whose safety envelope crosses into it. Wider jitter
    // makes that neighbor case intentional, not an extra-tree regression.
    let affected: std::collections::HashSet<_> = initial
        .iter()
        .filter_map(|(entity, _, transform)| {
            let p = transform.translation;
            let center = Vec2::new(p.x, -p.z);
            let radius = Vec2::splat(CROWN_RADIUS * transform.scale.x + 0.4);
            let min = ((center - radius) / 16.0).floor().as_ivec2();
            let max = ((center + radius) / 16.0).floor().as_ivec2();
            (parcel.cmpge(min).all() && parcel.cmple(max).all()).then_some(*entity)
        })
        .collect();
    assert!(affected.contains(&entity));
    assert!(
        affected.len() > 1,
        "fixture must exercise at least one neighboring crown"
    );
    let expected: Vec<_> = initial
        .iter()
        .copied()
        .filter(|(entity, _, _)| !affected.contains(entity))
        .collect();
    app.world_mut()
        .resource_mut::<ScenePointers>()
        .insert(parcel, occupied());
    app.update();
    for entity in affected {
        assert!(app.world().get_entity(entity).is_err());
    }
    assert_eq!(trees(&mut app), expected);
    let world = app.world_mut();
    *world
        .query_filtered::<&mut GlobalTransform, With<PrimaryUser>>()
        .single_mut(world)
        .unwrap() = GlobalTransform::from_translation(Vec3::splat(5000.0));
    app.update();
    assert!(trees(&mut app).is_empty());
    assert_eq!(collider_count(&mut app), 0);
}

#[test]
fn trunk_collision_uses_the_rendered_world_transform() {
    let transform = Transform::from_xyz(-25.0, 2.0, 40.0).with_scale(Vec3::splat(1.2));
    let mesh = trunk_collision().transformed_by(transform);
    let mut collision = SceneColliderData::from_terrain_mesh(&mesh).unwrap();
    assert!(collision.get_ground(Vec3::new(-25.0, 8.0, 40.0)).is_some());
    assert!(collision.get_ground(Vec3::new(-20.0, 8.0, 40.0)).is_none());
}
