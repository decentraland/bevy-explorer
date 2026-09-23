//! Recover arrivals underneath streamed terrain. A downward ground ray cannot
//! find a hill several metres above the avatar; normal capsule collision still
//! owns walking, slopes and jumps once the avatar is on the surface.
use crate::ground_visibility::GroundVisibility;
use bevy::prelude::*;
use common::{
    structs::{EngineMovementControl, LandscapeTerrain, PrimaryUser},
    terrain::TerrainField,
};
use scene_runner::{
    initialize_scene::{PointerResult, ScenePointers},
    vec3_to_parcel,
};

fn recovery_height(field: &TerrainField, position: Vec3) -> Option<f32> {
    if !position.is_finite() || !field.contains(position.x, -position.z) {
        return None;
    }
    let height = field.height(position.x, -position.z);
    // Do not fight ordinary contact resolution, jump arcs, or small foot overlap.
    (height - position.y > 0.35).then_some(height + 0.05)
}

pub(super) fn recover(
    terrain: Res<LandscapeTerrain>,
    visibility: Res<GroundVisibility>,
    pointers: Res<ScenePointers>,
    control: Res<EngineMovementControl>,
    mut player: Query<&mut Transform, With<PrimaryUser>>,
) {
    if !visibility.0
        || terrain.rendered_generation != Some(terrain.generation)
        || !control.suppress_avatar_physics.is_empty()
        || !control.suppress_clipping.is_empty()
        || control.deliberate_penetration
    {
        return;
    }
    let (Some(field), Ok(mut transform)) = (terrain.field.as_ref(), player.single_mut()) else {
        return;
    };
    if !matches!(
        pointers.get(vec3_to_parcel(transform.translation)),
        Some(PointerResult::Nothing)
    ) {
        return;
    }
    if let Some(height) = recovery_height(field, transform.translation) {
        transform.translation.y = height;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn fixture() -> (TerrainField, Vec3) {
        let field = TerrainField::from_parcels(&[[0, 0], [19, 19]], true).unwrap();
        let point = (1..18)
            .flat_map(|x| {
                (1..18)
                    .map(move |z| Vec3::new(x as f32 * 16.0 + 8.0, 0.0, -(z as f32 * 16.0 + 8.0)))
            })
            .find(|p| field.height(p.x, -p.z) > 3.0)
            .expect("fixture needs a raised hill");
        (field, point)
    }

    #[test]
    fn raises_buried_arrivals_but_does_not_pull_down_jumps_or_change_near_surface_contacts() {
        let (field, point) = fixture();
        let height = field.height(point.x, -point.z);
        assert_eq!(recovery_height(&field, point), Some(height + 0.05));
        assert_eq!(recovery_height(&field, point.with_y(height - 0.1)), None);
        assert_eq!(recovery_height(&field, point.with_y(height + 4.0)), None);
        assert_eq!(recovery_height(&field, Vec3::splat(100_000.0)), None);
        assert_eq!(recovery_height(&field, Vec3::splat(f32::NAN)), None);
    }

    #[test]
    fn recovery_respects_scene_ownership_visibility_streaming_and_explicit_physics_controls() {
        let (field, point) = fixture();
        let mut app = App::new();
        let mut pointers = ScenePointers::default();
        pointers.set_realm(IVec2::splat(-30), IVec2::splat(30));
        pointers.insert(vec3_to_parcel(point), PointerResult::Nothing);
        app.insert_resource(pointers)
            .insert_resource(GroundVisibility(true))
            .init_resource::<EngineMovementControl>()
            .insert_resource(LandscapeTerrain {
                generation: 1,
                rendered_generation: Some(1),
                field: Some(Arc::new(field)),
            })
            .add_systems(Update, recover);
        let player = app
            .world_mut()
            .spawn((PrimaryUser::default(), Transform::from_translation(point)))
            .id();
        app.update();
        let raised = app.world().get::<Transform>(player).unwrap().translation.y;
        assert!(raised > 3.0);
        let reset = |app: &mut App| {
            app.world_mut()
                .get_mut::<Transform>(player)
                .unwrap()
                .translation = point;
        };
        let height = |app: &App| app.world().get::<Transform>(player).unwrap().translation.y;
        reset(&mut app);
        app.world_mut()
            .resource_mut::<EngineMovementControl>()
            .deliberate_penetration = true;
        app.update();
        assert_eq!(height(&app), 0.0);
        app.world_mut()
            .resource_mut::<EngineMovementControl>()
            .deliberate_penetration = false;
        app.world_mut()
            .resource_mut::<EngineMovementControl>()
            .suppress_clipping
            .insert("test");
        app.update();
        assert_eq!(height(&app), 0.0);
        app.world_mut()
            .resource_mut::<EngineMovementControl>()
            .suppress_clipping
            .clear();
        app.world_mut()
            .resource_mut::<EngineMovementControl>()
            .suppress_avatar_physics
            .insert("test");
        app.update();
        assert_eq!(height(&app), 0.0);
        app.world_mut()
            .resource_mut::<EngineMovementControl>()
            .suppress_avatar_physics
            .clear();
        app.world_mut().resource_mut::<GroundVisibility>().0 = false;
        app.update();
        assert_eq!(height(&app), 0.0);
        app.world_mut().resource_mut::<GroundVisibility>().0 = true;
        app.world_mut()
            .resource_mut::<LandscapeTerrain>()
            .rendered_generation = None;
        app.update();
        assert_eq!(height(&app), 0.0);
        app.world_mut()
            .resource_mut::<LandscapeTerrain>()
            .rendered_generation = Some(1);
        app.world_mut().resource_mut::<ScenePointers>().insert(
            vec3_to_parcel(point),
            PointerResult::Exists {
                realm: "test".into(),
                hash: "scene".into(),
                urn: None,
            },
        );
        app.update();
        assert_eq!(height(&app), 0.0);
        *app.world_mut().resource_mut::<ScenePointers>() = ScenePointers::default();
        app.update();
        assert_eq!(height(&app), 0.0);
    }
}
