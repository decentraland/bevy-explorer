//! Colliders for solid landscape props (tree trunks, rocks) over the ground collider's patch,
//! rebuilt with it.

use bevy::{platform::collections::HashSet, prelude::*};
use common::sets::PostUpdateSets;
use dcl_component::SceneEntityId;
use scene_runner::update_world::mesh_collider::{ColliderId, SceneColliderData, SharedShape};

use crate::terrain::{update_collider, TerrainCollider};

pub(crate) struct SolidsPlugin;

impl Plugin for SolidsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            update_colliders
                .in_set(PostUpdateSets::ColliderUpdate)
                .after(update_collider),
        );
    }
}

/// Collision shape in the entity's local space.
#[derive(Component, Clone)]
pub(crate) enum Solid {
    Capsule {
        base: Vec3,
        top: Vec3,
        radius: f32,
    },
    /// Already at the entity's scale, so it can be shared.
    Shape(SharedShape),
}

fn collider_id(entity: Entity) -> ColliderId {
    ColliderId::new(SceneEntityId::ROOT, Some(format!("landscape {entity}")), 0)
}

/// Colliders for the solids over the ground collider's patch, rebuilt when the patch changes or
/// solids move.
fn update_colliders(
    collider: Single<(Ref<TerrainCollider>, &mut SceneColliderData)>,
    solids: Query<(Entity, &Solid, &Transform)>,
    moved: Query<(), (With<Solid>, Changed<Transform>)>,
    mut removed: RemovedComponents<Solid>,
    mut built: Local<HashSet<Entity>>,
) {
    let (patch, mut data) = collider.into_inner();
    for entity in removed.read() {
        if built.remove(&entity) {
            data.remove_collider(&collider_id(entity));
        }
    }
    if !patch.is_changed() && moved.is_empty() {
        return;
    }
    let Some((min, max)) = patch.patch() else {
        return;
    };
    let mut current = HashSet::new();
    for (entity, solid, transform) in &solids {
        let position = transform.translation.xz();
        if position.cmplt(min).any() || position.cmpgt(max).any() {
            continue;
        }
        let id = collider_id(entity);
        match solid {
            Solid::Capsule { base, top, radius } => data.set_physics_capsule(
                &id,
                transform.transform_point(*base),
                transform.transform_point(*top),
                radius * transform.scale.x,
            ),
            Solid::Shape(shape) => data.set_physics_shape(
                &id,
                shape.clone(),
                transform.translation,
                transform.rotation,
            ),
        }
        current.insert(entity);
    }
    for entity in built.difference(&current) {
        data.remove_collider(&collider_id(*entity));
    }
    *built = current;
}
