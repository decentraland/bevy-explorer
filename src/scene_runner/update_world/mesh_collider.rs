use bevy::prelude::*;
use rapier3d::prelude::*;

use crate::{
    dcl::interface::ComponentPosition,
    dcl_component::{
        proto_components::sdk::components::{pb_mesh_collider, PbMeshCollider},
        SceneComponentId, SceneEntityId,
    },
    scene_runner::{DeletedSceneEntities, RendererSceneContext, SceneEntity, SceneSets},
};

use super::{mesh_renderer::MeshDefinition, AddCrdtInterfaceExt};

pub struct MeshColliderPlugin;

#[derive(Component, Debug)]
pub enum MeshCollider {
    Box,
    Cylinder { radius_top: f32, radius_bottom: f32 },
    Plane,
    Sphere,
}

impl From<PbMeshCollider> for MeshCollider {
    fn from(value: PbMeshCollider) -> Self {
        match value.mesh {
            Some(pb_mesh_collider::Mesh::Box(_)) => Self::Box,
            Some(pb_mesh_collider::Mesh::Plane(_)) => Self::Plane,
            Some(pb_mesh_collider::Mesh::Sphere(_)) => Self::Sphere,
            Some(pb_mesh_collider::Mesh::Cylinder(pb_mesh_collider::CylinderMesh {
                radius_bottom,
                radius_top,
            })) => Self::Cylinder {
                radius_top: radius_top.unwrap_or(1.0),
                radius_bottom: radius_bottom.unwrap_or(1.0),
            },
            _ => Self::Box,
        }
    }
}

impl Plugin for MeshColliderPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbMeshCollider, MeshCollider>(
            SceneComponentId::MESH_COLLIDER,
            ComponentPosition::EntityOnly,
        );

        app.add_system(update_scene_collider_data.in_set(SceneSets::PostInit));
        app.add_system(update_colliders.in_set(SceneSets::PostLoop));

        // update collider transforms before queries and scenes are run, but after global transforms are updated (at end of prior frame)
        app.add_system(update_collider_transforms.in_set(SceneSets::PostInit));
    }
}

#[derive(Component, Default)]
pub struct SceneColliderData {
    collider_set: ColliderSet,
    entity_collider: bimap::BiMap<SceneEntityId, ColliderHandle>,
    query_state_valid_at: Option<f32>,
    query_state: Option<rapier3d::pipeline::QueryPipeline>,
    dummy_rapier_structs: (IslandManager, RigidBodySet),
}

impl SceneColliderData {
    pub fn set_collider(&mut self, id: SceneEntityId, new_collider: Collider) {
        self.remove_collider(id);

        let handle = self.collider_set.insert(new_collider);
        self.entity_collider.insert(id, handle);
        self.query_state_valid_at = None;
        debug!("set {id} collider");
    }

    pub fn update_collider_transform(&mut self, id: SceneEntityId, transform: &GlobalTransform) {
        if let Some(handle) = self.get_collider(id) {
            if let Some(collider) = self.collider_set.get_mut(handle) {
                collider.set_position(global_transform_to_iso(transform));
                debug!("update {id} collider");
            }
        }
        debug!("tried update {id} collider");
    }

    pub fn cast_ray_nearest(
        &mut self,
        scene_time: f32,
        origin: Vec3,
        direction: Vec3,
        distance: f32,
    ) -> Option<SceneEntityId> {
        if self.query_state_valid_at != Some(scene_time) {
            if self.query_state.is_none() {
                self.query_state = Some(Default::default());
            }
            self.query_state
                .as_mut()
                .unwrap()
                .update(&self.dummy_rapier_structs.1, &self.collider_set);
            self.query_state_valid_at = Some(scene_time);
        }

        let ray = rapier3d::prelude::Ray {
            origin: origin.into(),
            dir: direction.into(),
        };
        self.query_state
            .as_mut()
            .unwrap()
            .cast_ray(
                &self.dummy_rapier_structs.1,
                &self.collider_set,
                &ray,
                distance,
                false,
                QueryFilter::default(),
            )
            .and_then(|(handle, _)| self.get_entity(handle))
    }

    pub fn remove_collider(&mut self, id: SceneEntityId) {
        if let Some(handle) = self.entity_collider.get_by_left(&id) {
            self.collider_set.remove(
                *handle,
                &mut self.dummy_rapier_structs.0,
                &mut self.dummy_rapier_structs.1,
                false,
            );
        }
    }

    pub fn get_collider(&self, id: SceneEntityId) -> Option<ColliderHandle> {
        self.entity_collider.get_by_left(&id).copied()
    }

    pub fn get_entity(&self, handle: ColliderHandle) -> Option<SceneEntityId> {
        self.entity_collider.get_by_right(&handle).copied()
    }
}

fn update_scene_collider_data(
    mut commands: Commands,
    scenes: Query<Entity, (With<RendererSceneContext>, Without<SceneColliderData>)>,
) {
    for scene_ent in scenes.iter() {
        commands
            .entity(scene_ent)
            .insert(SceneColliderData::default());
    }
}

// collider state components
#[derive(Component)]
pub struct HasExplicitCollider;
#[derive(Component)]
pub struct HasInferredCollider;

#[allow(clippy::type_complexity)]
fn update_colliders(
    mut commands: Commands,
    // add explicit colliders
    // any entity with a mesh collider that we're not already using, or where the mesh collider has changed
    new_explicit_colliders: Query<
        (Entity, &SceneEntity, &MeshCollider),
        Or<(Changed<MeshCollider>, Without<HasExplicitCollider>)>,
    >,
    // add inferred colliders
    // any entity with a mesh definition that we're not using, or where the mesh definition has changed, as long as they don't have a mesh collider attached
    new_inferred_colliders: Query<
        (Entity, &SceneEntity, &MeshDefinition),
        (
            Or<(Changed<MeshDefinition>, Without<HasInferredCollider>)>,
            Without<MeshCollider>,
        ),
    >,
    // remove colliders
    // any entities with a live collider handle that don't have a mesh collider or a mesh definition
    colliders_without_source: Query<
        &SceneEntity,
        (
            Or<(With<HasExplicitCollider>, With<HasInferredCollider>)>,
            Without<MeshDefinition>,
            Without<MeshCollider>,
        ),
    >,
    // remove colliders for deleted entities
    mut scene_data: Query<(&mut SceneColliderData, Option<&DeletedSceneEntities>)>,
) {
    let mut update_collider = |scene_ent: &SceneEntity, new_collider: Collider| {
        let Ok((mut scene_data, _)) = scene_data.get_mut(scene_ent.root) else {
            warn!("missing scene root for {scene_ent:?}");
            return;
        };

        scene_data.set_collider(scene_ent.id, new_collider);
    };

    // add explicit colliders
    // any entity with a mesh collider that we're not using, or where the mesh collider has changed
    for (ent, scene_ent, collider) in new_explicit_colliders.iter() {
        let collider = match collider {
            MeshCollider::Box => ColliderBuilder::cuboid(0.5, 0.5, 0.5),
            MeshCollider::Cylinder { .. } => unimplemented!(),
            MeshCollider::Plane => ColliderBuilder::cuboid(0.5, 0.05, 0.5),
            MeshCollider::Sphere => ColliderBuilder::ball(0.5),
        }
        .build();

        update_collider(scene_ent, collider);
        commands
            .entity(ent)
            .remove::<HasExplicitCollider>()
            .insert(HasInferredCollider);
    }

    // add inferred colliders
    // any entity with a mesh definition that we're not using, or where the mesh definition has changed, as long as they don't have a mesh collider attached
    for (ent, scene_ent, mesh_definition) in new_inferred_colliders.iter() {
        let collider = match mesh_definition {
            MeshDefinition::Box { .. } => ColliderBuilder::cuboid(0.5, 0.5, 0.5),
            MeshDefinition::Cylinder { .. } => unimplemented!(),
            MeshDefinition::Plane { .. } => ColliderBuilder::cuboid(0.5, 0.05, 0.5),
            MeshDefinition::Sphere => ColliderBuilder::ball(0.5),
        }
        .build();

        update_collider(scene_ent, collider);
        commands
            .entity(ent)
            .remove::<HasInferredCollider>()
            .insert(HasExplicitCollider);
    }

    // remove colliders
    // any entities with a live collider handle that don't have a mesh collider or a mesh definition
    for scene_ent in colliders_without_source.iter() {
        let Ok((mut scene_data, _)) = scene_data.get_mut(scene_ent.root) else {
            warn!("missing scene root for {scene_ent:?}");
            continue;
        };

        scene_data.remove_collider(scene_ent.id);
    }

    // remove colliders for deleted entities
    for (mut scene_data, maybe_deleted_entities) in &mut scene_data {
        if let Some(deleted_entities) = maybe_deleted_entities {
            for deleted_entity in &deleted_entities.0 {
                scene_data.remove_collider(*deleted_entity);
            }
        }
    }
}

#[allow(clippy::type_complexity)]
fn update_collider_transforms(
    changed_colliders: Query<
        (&SceneEntity, &GlobalTransform),
        (
            Or<(With<HasExplicitCollider>, With<HasInferredCollider>)>, // has some collider
            Or<(
                Changed<GlobalTransform>,
                Added<HasExplicitCollider>,
                Added<HasInferredCollider>,
            )>, // which needs updating
        ),
    >,
    mut scene_data: Query<&mut SceneColliderData>,
) {
    for (scene_ent, global_transform) in changed_colliders.iter() {
        let Ok(mut scene_data) = scene_data.get_mut(scene_ent.root) else {
            warn!("missing scene root for {scene_ent:?}");
            continue;
        };

        scene_data.update_collider_transform(scene_ent.id, global_transform);
    }
}

pub fn global_transform_to_iso(global_transform: &GlobalTransform) -> Isometry<Real> {
    let (_, rotation, translation) = global_transform.to_scale_rotation_translation();
    Isometry::from_parts(translation.into(), rotation.into())
}
