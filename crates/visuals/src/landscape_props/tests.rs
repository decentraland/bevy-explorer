use super::*;
use std::sync::Arc;

fn fixture() -> (TerrainField, ScenePointers) {
    let field = TerrainField::from_parcels(&[[0, 0]], true).unwrap();
    let mut pointers = ScenePointers::default();
    pointers.set_realm(IVec2::splat(-10), IVec2::splat(10));
    for x in -10..=10 {
        for y in -10..=10 {
            pointers.insert(IVec2::new(x, y), PointerResult::Nothing);
        }
    }
    (field, pointers)
}

#[test]
fn props_are_deterministic_anchored_and_never_cross_occupied_parcels() {
    let (field, pointers) = fixture();
    let mut kinds = [0; KINDS];
    for x in -2..=2 {
        for y in -2..=2 {
            let parcel = IVec2::new(x, y);
            let props = placements(parcel, &field, &pointers);
            assert_eq!(props, placements(parcel, &field, &pointers));
            if parcel == IVec2::ZERO {
                assert!(props.is_empty());
            }
            for prop in props {
                kinds[prop.kind] += 1;
                let p = prop.transform.translation;
                assert_eq!(vec3_to_parcel(p), parcel);
                assert!((p.y + 0.015 - field.height(p.x, -p.z)).abs() < 0.0001);
                let center = parcel.as_vec2() * 16.0 + Vec2::splat(8.0);
                assert!(Vec2::new(p.x, -p.z).distance(center) >= 2.0);
                if prop.kind < 2 {
                    assert!(Vec2::new(p.x, -p.z).distance(center) >= 2.0 + prop.transform.scale.x);
                    let local = Vec2::new(p.x, -p.z) - parcel.as_vec2() * 16.0;
                    let radius = prop.transform.scale.x;
                    assert!(local.min_element() >= radius);
                    assert!(local.max_element() <= 16.0 - radius);
                }
            }
        }
    }
    assert!(kinds.iter().all(|&count| count > 0));
    assert!(placements(IVec2::NEG_ONE, &field, &ScenePointers::default()).is_empty());
    assert!(placements(IVec2::splat(600), &field, &pointers).is_empty());
}

#[test]
fn shrubs_group_around_valid_rocks_without_adding_slots_or_crossing_insets() {
    let (_, pointers) = fixture();
    // A single occupied parcel creates only a 5 x 5 terrain rectangle. Span
    // the sample region so these candidates exercise real in-bounds terrain,
    // including slopes; keep the production slope/arrival rejection unchanged.
    let field = TerrainField::from_parcels(&[[-8, -8], [8, 8]], true).unwrap();
    let mut grouped = 0;
    for x in -10..=10 {
        for y in -10..=10 {
            let props = placements(IVec2::new(x, y), &field, &pointers);
            assert!(props.len() <= 9);
            let mut slots = std::collections::HashSet::new();
            for prop in &props {
                assert!(slots.insert(prop.key.slot));
                let p = prop.transform.translation;
                let local = Vec2::new(p.x, -p.z) - prop.key.parcel.as_vec2() * 16.0;
                assert!(local.min_element() >= 2.5 && local.max_element() <= 13.5);
            }
            let Some(rock) = props.iter().find(|prop| prop.kind < 2) else {
                continue;
            };
            for bush in props.iter().filter(|prop| (2..4).contains(&prop.kind)) {
                let distance = (bush.transform.translation - rock.transform.translation)
                    .xz()
                    .length();
                assert!(distance >= rock.transform.scale.x + 0.999);
                assert!(distance <= rock.transform.scale.x + 1.601);
                grouped += 1;
            }
        }
    }
    assert!(grouped > 12, "fixture must exercise rock/shrub groups");
}

#[test]
fn off_restore_realm_switch_and_opt_out_clear_geometry_and_collision() {
    let (field, pointers) = fixture();
    let mut app = App::new();
    app.init_resource::<bevy::asset::Assets<Mesh>>()
        .init_resource::<bevy::asset::Assets<Image>>()
        .init_resource::<bevy::asset::Assets<crate::landscape_trees::TreeMaterial>>()
        .init_resource::<bevy::asset::Assets<crate::landscape_rigid::RigidMaterial>>()
        .init_resource::<State>()
        .insert_resource(GroundVisibility(true))
        .insert_resource(LandscapeTerrain {
            generation: 1,
            field: Some(Arc::new(field)),
            ..default()
        })
        .insert_resource(pointers)
        .add_systems(
            Startup,
            (
                crate::landscape_trees::setup,
                crate::landscape_rigid::setup,
                setup,
            ),
        )
        .add_systems(Update, refresh);
    app.world_mut().spawn((
        PrimaryUser::default(),
        GlobalTransform::from_translation(Vec3::new(-24.0, 0.0, 24.0)),
    ));
    let count = |app: &mut App| app.world_mut().query::<&Prop>().iter(app.world()).count();
    let colliders = |app: &mut App| {
        app.world_mut()
            .query::<&RockColliders>()
            .iter(app.world())
            .count()
    };
    app.update();
    let initial = count(&mut app);
    assert!(initial > 0);
    assert_eq!(colliders(&mut app), 1);
    for (prop, rigid, foliage) in app
        .world_mut()
        .query::<(
            &Prop,
            Option<&MeshMaterial3d<crate::landscape_rigid::RigidMaterial>>,
            Option<&MeshMaterial3d<crate::landscape_trees::TreeMaterial>>,
        )>()
        .iter(app.world())
    {
        assert_eq!(rigid.is_some(), prop.kind < 2);
        assert_eq!(foliage.is_some(), prop.kind >= 2);
    }
    {
        let mut state = app.world_mut().resource_mut::<State>();
        state.enabled = false;
        state.dirty = true;
    }
    app.update();
    assert_eq!(count(&mut app), 0);
    assert_eq!(colliders(&mut app), 0);
    {
        let mut state = app.world_mut().resource_mut::<State>();
        state.enabled = true;
        state.dirty = true;
    }
    app.update();
    assert_eq!(count(&mut app), initial);
    app.world_mut().resource_mut::<GroundVisibility>().0 = false;
    app.update();
    assert_eq!(count(&mut app), 0);
    assert_eq!(colliders(&mut app), 0);
    app.world_mut().resource_mut::<GroundVisibility>().0 = true;
    app.update();
    assert_eq!(count(&mut app), initial);
    {
        let mut terrain = app.world_mut().resource_mut::<LandscapeTerrain>();
        terrain.generation += 1;
        terrain.field = None;
    }
    app.update();
    assert_eq!(count(&mut app), 0);
    assert_eq!(colliders(&mut app), 0);
    assert_eq!(
        app.world().resource::<bevy::asset::Assets<Mesh>>().len(),
        KINDS * 2 + crate::tree_geometry::KINDS * 3
    );
}
