use bevy::{prelude::*, utils::HashMap};
use rapier3d::prelude::*;

use crate::{
    dcl::interface::ComponentPosition,
    dcl_component::{
        proto_components::sdk::components::{pb_mesh_collider, ColliderLayer, PbMeshCollider},
        SceneComponentId, SceneEntityId,
    },
    scene_runner::{DeletedSceneEntities, RendererSceneContext, SceneEntity, SceneSets},
};

use super::AddCrdtInterfaceExt;

pub struct MeshColliderPlugin;

#[derive(Component, Debug)]
pub struct MeshCollider {
    shape: MeshColliderShape,
    collision_mask: u32,
}

#[derive(Debug)]
pub enum MeshColliderShape {
    Box,
    Cylinder { radius_top: f32, radius_bottom: f32 },
    Plane,
    Sphere,
}

impl From<PbMeshCollider> for MeshCollider {
    fn from(value: PbMeshCollider) -> Self {
        let shape = match value.mesh {
            Some(pb_mesh_collider::Mesh::Box(_)) => MeshColliderShape::Box,
            Some(pb_mesh_collider::Mesh::Plane(_)) => MeshColliderShape::Plane,
            Some(pb_mesh_collider::Mesh::Sphere(_)) => MeshColliderShape::Sphere,
            Some(pb_mesh_collider::Mesh::Cylinder(pb_mesh_collider::CylinderMesh {
                radius_bottom,
                radius_top,
            })) => MeshColliderShape::Cylinder {
                radius_top: radius_top.unwrap_or(1.0),
                radius_bottom: radius_bottom.unwrap_or(1.0),
            },
            _ => MeshColliderShape::Box,
        };

        Self {
            shape,
            // TODO update to u32
            collision_mask: value
                .collision_mask
                .unwrap_or(ColliderLayer::ClPointer as i32 | ColliderLayer::ClPhysics as i32)
                as u32,
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

#[derive(Debug)]
pub struct RaycastResult {
    pub id: SceneEntityId,
    pub toi: f32,
    pub normal: Vec3,
}

struct ColliderState {
    base_collider: Collider,
    scale: Vec3,
}

#[derive(Component, Default)]
pub struct SceneColliderData {
    collider_set: ColliderSet,
    scaled_collider: bimap::BiMap<SceneEntityId, ColliderHandle>,
    collider_state: HashMap<SceneEntityId, ColliderState>,
    query_state_valid_at: Option<f32>,
    query_state: Option<rapier3d::pipeline::QueryPipeline>,
    dummy_rapier_structs: (IslandManager, RigidBodySet),
}

const SCALE_EPSILON: f32 = 0.001;

impl SceneColliderData {
    pub fn set_collider(&mut self, id: SceneEntityId, new_collider: Collider) {
        self.remove_collider(id);

        self.collider_state.insert(
            id,
            ColliderState {
                base_collider: new_collider.clone(),
                scale: Vec3::ONE,
            },
        );
        let handle = self.collider_set.insert(new_collider);
        self.scaled_collider.insert(id, handle);
        self.query_state_valid_at = None;
        debug!("set {id} collider");
    }

    pub fn update_collider_transform(&mut self, id: SceneEntityId, transform: &GlobalTransform) {
        if let Some(handle) = self.get_collider(id) {
            if let Some(collider) = self.collider_set.get_mut(handle) {
                let (req_scale, rotation, translation) = transform.to_scale_rotation_translation();
                let ColliderState {
                    base_collider,
                    scale,
                } = self.collider_state.get(&id).unwrap();

                // colliders don't have a scale, we have to modify the shape directly when scale changes (significantly)
                if (req_scale - *scale).length_squared() > SCALE_EPSILON {
                    let base_shape = base_collider.shape();
                    match base_shape.as_typed_shape() {
                        TypedShape::Ball(b) => match b.scaled(&req_scale.into(), 5).unwrap() {
                            itertools::Either::Left(ball) => {
                                collider.set_shape(SharedShape::new(ball))
                            }
                            itertools::Either::Right(convex) => {
                                collider.set_shape(SharedShape::new(convex))
                            }
                        },
                        TypedShape::Cuboid(c) => {
                            collider.set_shape(SharedShape::new(c.scaled(&req_scale.into())))
                        }
                        _ => unimplemented!(),
                    };
                }
                self.collider_state.get_mut(&id).unwrap().scale = req_scale;

                collider.set_position(Isometry::from_parts(translation.into(), rotation.into()));
            }
        }
    }

    fn update_pipeline(&mut self, scene_time: f32) {
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
    }

    pub fn cast_ray_nearest(
        &mut self,
        scene_time: f32,
        origin: Vec3,
        direction: Vec3,
        distance: f32,
        collision_mask: u32,
    ) -> Option<RaycastResult> {
        let ray = rapier3d::prelude::Ray {
            origin: origin.into(),
            dir: direction.into(),
        };
        self.update_pipeline(scene_time);

        self.query_state
            .as_ref()
            .unwrap()
            .cast_ray_and_get_normal(
                &self.dummy_rapier_structs.1,
                &self.collider_set,
                &ray,
                distance,
                true,
                QueryFilter::default().groups(InteractionGroups::new(
                    Group::from_bits_truncate(collision_mask),
                    Group::all(),
                )),
            )
            .map(|(handle, intersection)| RaycastResult {
                id: self.get_entity(handle).unwrap(),
                toi: intersection.toi,
                normal: Vec3::from(intersection.normal),
            })
    }

    pub fn cast_ray_all(
        &mut self,
        scene_time: f32,
        origin: Vec3,
        direction: Vec3,
        distance: f32,
        collision_mask: u32,
    ) -> Vec<RaycastResult> {
        let ray = rapier3d::prelude::Ray {
            origin: origin.into(),
            dir: direction.into(),
        };
        let mut results = Vec::default();
        self.update_pipeline(scene_time);

        self.query_state.as_ref().unwrap().intersections_with_ray(
            &self.dummy_rapier_structs.1,
            &self.collider_set,
            &ray,
            distance,
            true,
            QueryFilter::default().groups(InteractionGroups::new(
                Group::from_bits_truncate(collision_mask),
                Group::all(),
            )),
            |handle, intersection| {
                results.push(RaycastResult {
                    id: self.get_entity(handle).unwrap(),
                    toi: intersection.toi,
                    normal: Vec3::from(intersection.normal),
                });
                true
            },
        );

        results
    }

    pub fn remove_collider(&mut self, id: SceneEntityId) {
        if let Some(handle) = self.scaled_collider.get_by_left(&id) {
            self.collider_set.remove(
                *handle,
                &mut self.dummy_rapier_structs.0,
                &mut self.dummy_rapier_structs.1,
                false,
            );
        }

        self.scaled_collider.remove_by_left(&id);
        self.collider_state.remove(&id);
    }

    pub fn get_collider(&self, id: SceneEntityId) -> Option<ColliderHandle> {
        self.scaled_collider.get_by_left(&id).copied()
    }

    pub fn get_entity(&self, handle: ColliderHandle) -> Option<SceneEntityId> {
        self.scaled_collider.get_by_right(&handle).copied()
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

// collider state component
#[derive(Component)]
pub struct HasCollider;

#[allow(clippy::type_complexity)]
fn update_colliders(
    mut commands: Commands,
    // add colliders
    // any entity with a mesh collider that we're not already using, or where the mesh collider has changed
    new_colliders: Query<
        (Entity, &SceneEntity, &MeshCollider),
        Or<(Changed<MeshCollider>, Without<HasCollider>)>,
    >,
    // remove colliders
    // any entities with a live collider handle that don't have a mesh collider
    colliders_without_source: Query<
        (Entity, &SceneEntity),
        (With<HasCollider>, Without<MeshCollider>),
    >,
    // remove colliders for deleted entities
    mut scene_data: Query<(&mut SceneColliderData, Option<&DeletedSceneEntities>)>,
) {
    // add colliders
    // any entity with a mesh collider that we're not using, or where the mesh collider has changed
    for (ent, scene_ent, collider_def) in new_colliders.iter() {
        let collider = match collider_def.shape {
            MeshColliderShape::Box => ColliderBuilder::cuboid(0.5, 0.5, 0.5),
            MeshColliderShape::Cylinder { .. } => {
                warn!("cylinder not implemented");
                ColliderBuilder::ball(0.5)
            }
            MeshColliderShape::Plane => ColliderBuilder::cuboid(0.5, 0.05, 0.5),
            MeshColliderShape::Sphere => ColliderBuilder::ball(0.5),
        }
        .collision_groups(InteractionGroups {
            memberships: Group::from_bits_truncate(collider_def.collision_mask),
            filter: Group::all(),
        })
        .build();

        debug!("{} adding collider", scene_ent.id);
        let Ok((mut scene_data, _)) = scene_data.get_mut(scene_ent.root) else {
            warn!("missing scene root for {scene_ent:?}");
            continue;
        };

        scene_data.set_collider(scene_ent.id, collider);
        commands.entity(ent).insert(HasCollider);
    }

    // remove colliders
    // any entities with a live collider handle that don't have a mesh collider or a mesh definition
    for (ent, scene_ent) in colliders_without_source.iter() {
        let Ok((mut scene_data, _)) = scene_data.get_mut(scene_ent.root) else {
            warn!("missing scene root for {scene_ent:?}");
            continue;
        };

        scene_data.remove_collider(scene_ent.id);
        commands.entity(ent).remove::<HasCollider>();
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
            With<HasCollider>,                                  // has some collider
            Or<(Changed<GlobalTransform>, Added<HasCollider>)>, // which needs updating
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
