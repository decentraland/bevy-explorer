use super::*;
use common::terrain::TerrainField;
use std::sync::Arc;

fn count<T: Component>(app: &mut App) -> usize {
    app.world_mut()
        .query_filtered::<Entity, With<T>>()
        .iter(app.world())
        .count()
}

#[test]
fn coast_reuses_water_and_clears_all_geometry_and_collision_on_opt_out_or_realm_change() {
    let mut app = App::new();
    app.init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<Image>>()
        .init_resource::<Assets<WaterMaterial>>()
        .init_resource::<Assets<crate::landscape_rigid::RigidMaterial>>()
        .init_resource::<State>()
        .insert_resource(GroundVisibility(true))
        .insert_resource(LandscapeTerrain {
            generation: 1,
            field: Some(Arc::new(
                TerrainField::from_parcels(&[[0, 0]], true).unwrap(),
            )),
            ..default()
        })
        .add_systems(Startup, (crate::landscape_rigid::setup, setup))
        .add_systems(Update, update);
    let camera = app
        .world_mut()
        .spawn((PrimaryCamera::default(), GlobalTransform::IDENTITY))
        .id();
    app.update();
    assert_eq!(count::<Ocean>(&mut app), 1);
    assert_eq!(count::<Border>(&mut app), 1);
    assert_eq!(count::<SceneColliderData>(&mut app), 1);
    let water = app.world().resource::<WaterAssets>();
    let materials = app.world().resource::<Assets<WaterMaterial>>();
    let material = materials.get(&water.material).unwrap();
    let image = app
        .world()
        .resource::<Assets<Image>>()
        .get(&material.extension.ripples)
        .unwrap();
    let ShaderRef::Path(shader) = Water::fragment_shader() else {
        panic!("default ocean must use the native wave shader");
    };
    assert!(shader.to_string().ends_with("landscape_water.wgsl"));
    assert_eq!(
        image.data,
        crate::ocean_texture::image().data,
        "ordinary coast startup must use the generated irregular ripples"
    );
    assert_eq!(image.texture_descriptor.mip_level_count, 9);
    assert_eq!(material.base.alpha_mode, AlphaMode::Opaque);
    assert!(
        !material.base.fog_enabled,
        "open ocean must not become a flat fog slab"
    );
    let ripple_handle = material.extension.ripples.clone();
    let initial_cliffs = count::<Cliff>(&mut app);
    assert!(initial_cliffs > 0);
    assert_eq!(
        count::<MeshMaterial3d<crate::landscape_rigid::RigidMaterial>>(&mut app),
        initial_cliffs
    );
    let ocean = app
        .world_mut()
        .query_filtered::<Entity, With<Ocean>>()
        .single(app.world())
        .unwrap();
    app.world_mut()
        .entity_mut(camera)
        .insert(GlobalTransform::from_translation(Vec3::new(
            256.0, 0.0, 0.0,
        )));
    app.update();
    assert!(
        app.world().get::<Ocean>(ocean).is_some(),
        "Moving the camera must reuse the ocean"
    );
    assert_eq!(
        app.world().get::<Transform>(ocean).unwrap().translation,
        Vec3::new(256.0, SEA_LEVEL, 0.0)
    );
    assert_eq!(count::<Border>(&mut app), 1);
    let water = app.world().resource::<WaterAssets>();
    assert_eq!(
        app.world()
            .resource::<Assets<WaterMaterial>>()
            .get(&water.material)
            .unwrap()
            .extension
            .ripples,
        ripple_handle
    );
    app.world_mut().resource_mut::<GroundVisibility>().0 = false;
    app.update();
    assert_eq!(count::<Ocean>(&mut app), 0);
    assert_eq!(count::<Cliff>(&mut app), 0);
    assert_eq!(count::<SceneColliderData>(&mut app), 0);
    app.world_mut().resource_mut::<GroundVisibility>().0 = true;
    app.update();
    assert_eq!(count::<Ocean>(&mut app), 1);
    assert_eq!(count::<Cliff>(&mut app), initial_cliffs);
    assert_eq!(count::<SceneColliderData>(&mut app), 1);
    {
        let mut terrain = app.world_mut().resource_mut::<LandscapeTerrain>();
        terrain.generation += 1;
        terrain.field = None;
    }
    app.update();
    assert_eq!(count::<Ocean>(&mut app), 0);
    assert_eq!(count::<Cliff>(&mut app), 0);
    assert_eq!(count::<Border>(&mut app), 0);
    assert_eq!(count::<SceneColliderData>(&mut app), 0);
}

#[test]
fn borders_are_outside_the_playable_rectangle_and_cover_all_four_sides() {
    use bevy::render::mesh::VertexAttributeValues;
    let bounds = Vec4::new(-128.0, -64.0, 256.0, 512.0);
    let mesh = geometry::boundary_collider(bounds);
    let Some(VertexAttributeValues::Float32x3(points)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        panic!("positions missing")
    };
    assert!(points
        .iter()
        .all(|p| p[0] <= bounds.x || p[0] >= bounds.z || -p[2] <= bounds.y || -p[2] >= bounds.w));
    for target in [
        Vec3::new(bounds.x, 0.0, -bounds.y),
        Vec3::new(bounds.z, 0.0, -bounds.y),
        Vec3::new(bounds.x, 0.0, -bounds.w),
        Vec3::new(bounds.z, 0.0, -bounds.w),
    ] {
        assert!(points
            .iter()
            .any(|p| Vec3::from_array(*p).distance(target) < 0.001));
    }
    assert!(SceneColliderData::from_terrain_mesh(&mesh).is_ok());
}

#[test]
fn sand_and_ocean_share_the_round_shoreline_and_advancing_wash() {
    let shader = include_str!("../landscape_water.wgsl");
    assert_eq!(shader.matches("textureSample(").count(), 2);
    assert!(shader.contains("coast_surf(unity, land_bounds, globals.time, footprint)"));
    assert_eq!(shader.matches("var ripples: texture_2d<f32>").count(), 1);
    let beach = include_str!("../landscape_rigid.wgsl");
    assert!(beach.contains("coast_surf(p * vec2(1.0, -1.0), land_bounds, globals.time, footprint)"));
    let wash = include_str!("../coast_surf.wgsl");
    assert!(wash.contains("coast_distance(unity, bounds)"));
    assert!(wash.contains("0.8 - 4.4 * pulse * pulse"));
    assert!(wash.contains("surf_noise(unity * 0.32"));
    assert!(wash.contains("surf_noise(unity * 1.7"));
    assert!(wash.contains("smoothstep(0.15, 0.65, footprint)"));
    assert!(wash.contains("smoothstep(-6.0, -0.5, distance)"));
    assert!(!wash.contains("textureSample"));
}
