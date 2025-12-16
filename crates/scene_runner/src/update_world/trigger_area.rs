use bevy::{platform::collections::HashMap, prelude::*, render::mesh::VertexAttributeValues};
use common::sets::SceneSets;
use dcl::interface::{ComponentPosition, CrdtType};
use dcl_component::{
    proto_components::{
        common::Vector3,
        sdk::components::{
            pb_trigger_area_result::Trigger, ColliderLayer, PbTriggerArea, PbTriggerAreaResult,
            TriggerAreaEventType, TriggerAreaMeshType,
        },
    },
    SceneComponentId,
};
use rapier3d::{
    math::Point,
    prelude::{ColliderBuilder, SharedShape},
};

use crate::{
    gltf_resolver::GltfMeshResolver,
    renderer_context::RendererSceneContext,
    update_world::{
        gltf_container::mesh_to_parry_shape,
        mesh_collider::{ColliderId, MeshCollider, MeshColliderShape, SceneColliderData},
        mesh_renderer::truncated_cone::TruncatedCone,
        AddCrdtInterfaceExt,
    },
    ContainerEntity,
};

#[derive(Component)]
pub struct TriggerArea {
    pub shape: MeshColliderShape,
    pub trigger_mask: u32,
    pub mesh_name: Option<String>,
    pub index: u32,
}

impl Default for TriggerArea {
    fn default() -> Self {
        Self {
            shape: MeshColliderShape::Box,
            trigger_mask: ColliderLayer::ClPlayer as u32,
            mesh_name: Default::default(),
            index: Default::default(),
        }
    }
}

impl From<PbTriggerArea> for TriggerArea {
    fn from(value: PbTriggerArea) -> Self {
        let shape = match value.mesh() {
            TriggerAreaMeshType::TamtBox => MeshColliderShape::Box,
            TriggerAreaMeshType::TamtSphere => MeshColliderShape::Sphere,
        };

        Self {
            shape,
            trigger_mask: value
                .collision_mask
                .unwrap_or(ColliderLayer::ClPlayer as u32),
            ..Default::default()
        }
    }
}

pub struct TriggerAreaPlugin;

impl Plugin for TriggerAreaPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbTriggerArea, TriggerArea>(
            SceneComponentId::TRIGGER_AREA,
            ComponentPosition::EntityOnly,
        );

        app.add_systems(
            Update,
            (update_trigger_shapes, update_triggers)
                .chain()
                .in_set(SceneSets::Input),
        );
    }
}

#[derive(Component)]
pub struct TriggerShape(SharedShape);

fn update_trigger_shapes(
    mut commands: Commands,
    changed_triggers: Query<(Entity, &ContainerEntity, &TriggerArea), Changed<TriggerArea>>,
    scenes: Query<&RendererSceneContext>,
    mut gltf_mesh_resolver: GltfMeshResolver,
    meshes: Res<Assets<Mesh>>,
) {
    for (entity, container, area) in changed_triggers.iter() {
        let shape = match &area.shape {
            MeshColliderShape::Box => ColliderBuilder::cuboid(0.5, 0.5, 0.5),
            MeshColliderShape::Cylinder {
                radius_top,
                radius_bottom,
            } => {
                // TODO we could use explicit support points to make queries faster
                let mesh: Mesh = TruncatedCone {
                    base_radius: *radius_bottom,
                    tip_radius: *radius_top,
                    ..Default::default()
                }
                .into();
                let VertexAttributeValues::Float32x3(positions) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
                else {
                    panic!()
                };
                ColliderBuilder::convex_hull(
                    &positions
                        .iter()
                        .map(|p| Point::from([p[0], p[1], p[2]]))
                        .collect::<Vec<_>>(),
                )
                .unwrap()
            }
            MeshColliderShape::Plane => ColliderBuilder::cuboid(0.5, 0.5, 0.005),
            MeshColliderShape::Sphere => ColliderBuilder::ball(0.5),
            MeshColliderShape::Shape(shape, _) => ColliderBuilder::new(shape.clone()),
            MeshColliderShape::GltfShape { gltf_src, name } => {
                let Ok(scene) = scenes.get(container.root) else {
                    continue;
                };
                let Ok(Some(h_mesh)) = gltf_mesh_resolver.resolve_mesh(gltf_src, &scene.hash, name)
                else {
                    continue;
                };
                let mesh = meshes.get(&h_mesh).unwrap();
                let shape = mesh_to_parry_shape(mesh);
                ColliderBuilder::new(shape)
            }
        }
        .build()
        .shared_shape()
        .clone();

        commands.entity(entity).try_insert(TriggerShape(shape));
    }
}

#[derive(Component)]
pub struct ActiveTriggers(pub HashMap<ColliderId, u32>);

fn update_triggers(
    mut commands: Commands,
    trigger_areas: Query<(
        Entity,
        &ContainerEntity,
        Ref<TriggerArea>,
        Option<&ActiveTriggers>,
        &TriggerShape,
        &GlobalTransform,
    )>,
    mut scenes: Query<(&mut RendererSceneContext, &mut SceneColliderData)>,
    triggers: Query<(&MeshCollider, &GlobalTransform)>,
) {
    for (entity, container, trigger, maybe_active, shape, gt) in trigger_areas.iter() {
        let Ok((mut scene, mut colliders)) = scenes.get_mut(container.root) else {
            continue;
        };

        let timestamp = scene.last_update_frame;

        // get intersecting colliders
        let new_colliders =
            colliders.intersect_shape(scene.last_update_frame, &shape.0, gt, trigger.trigger_mask);
        let (_, rotation, translation) = gt.to_scale_rotation_translation();

        let mut results = Vec::default();
        if let Some(prev_active) = maybe_active.as_ref() {
            for (prev_collider, prev_frame) in &prev_active.0 {
                let trigger = colliders
                    .get_collider_entity(prev_collider)
                    .and_then(|e| triggers.get(e).ok())
                    .map(|(collider, gt)| {
                        let (s, r, t) = gt.to_scale_rotation_translation();
                        Trigger {
                            entity: prev_collider.entity.as_proto_u32().unwrap(),
                            layers: collider.collision_mask,
                            position: Some(Vector3::world_vec_from_vec3(&t)),
                            rotation: Some(r.into()),
                            scale: Some(Vector3::abs_vec_from_vec3(&s)),
                        }
                    })
                    .unwrap_or_else(|| Trigger {
                        entity: prev_collider.entity.as_proto_u32().unwrap(),
                        layers: 0,
                        position: None,
                        rotation: None,
                        scale: None,
                    });

                if new_colliders.contains(prev_collider) {
                    if prev_frame != &timestamp {
                        // send only 1 stay per scene tick
                        results.push((
                            container.container_id,
                            PbTriggerAreaResult {
                                triggered_entity: container.container_id.as_proto_u32().unwrap(),
                                triggered_entity_position: Some(Vector3::world_vec_from_vec3(
                                    &translation,
                                )),
                                triggered_entity_rotation: Some(rotation.into()),
                                event_type: TriggerAreaEventType::TaetStay as i32,
                                timestamp,
                                trigger: Some(trigger.clone()),
                            },
                        ));
                    }
                } else {
                    results.push((
                        container.container_id,
                        PbTriggerAreaResult {
                            triggered_entity: container.container_id.as_proto_u32().unwrap(),
                            triggered_entity_position: Some(Vector3::world_vec_from_vec3(
                                &translation,
                            )),
                            triggered_entity_rotation: Some(rotation.into()),
                            event_type: TriggerAreaEventType::TaetExit as i32,
                            timestamp,
                            trigger: Some(trigger.clone()),
                        },
                    ));
                }
            }
        }

        for new_collider in &new_colliders {
            if maybe_active
                .as_ref()
                .is_none_or(|prev_active| !prev_active.0.contains_key(new_collider))
            {
                let trigger = colliders
                    .get_collider_entity(new_collider)
                    .and_then(|e| triggers.get(e).ok())
                    .map(|(collider, gt)| {
                        let (s, r, t) = gt.to_scale_rotation_translation();
                        Trigger {
                            entity: new_collider.entity.as_proto_u32().unwrap(),
                            layers: collider.collision_mask,
                            position: Some(Vector3::world_vec_from_vec3(&t)),
                            rotation: Some(r.into()),
                            scale: Some(Vector3::abs_vec_from_vec3(&s)),
                        }
                    })
                    .unwrap_or_else(|| Trigger {
                        entity: new_collider.entity.as_proto_u32().unwrap(),
                        layers: 0,
                        position: None,
                        rotation: None,
                        scale: None,
                    });

                results.push((
                    container.container_id,
                    PbTriggerAreaResult {
                        triggered_entity: container.container_id.as_proto_u32().unwrap(),
                        triggered_entity_position: Some(Vector3::world_vec_from_vec3(&translation)),
                        triggered_entity_rotation: Some(rotation.into()),
                        event_type: TriggerAreaEventType::TaetEnter as i32,
                        timestamp,
                        trigger: Some(trigger.clone()),
                    },
                ));
            }
        }

        for (scene_ent, trigger) in results {
            scene.update_crdt(
                SceneComponentId::TRIGGER_AREA_RESULT,
                CrdtType::GO_ENT,
                scene_ent,
                &trigger,
            );
        }

        commands.entity(entity).try_insert(ActiveTriggers(
            new_colliders.into_iter().map(|c| (c, timestamp)).collect(),
        ));
    }
}
