use bevy::{diagnostic::FrameCount, platform::collections::HashMap, prelude::*};
use common::{sets::SceneSets, structs::MonotonicTimestamp};
use dcl::interface::{ComponentPosition, CrdtType};
use dcl_component::{
    proto_components::{
        common::Vector3,
        sdk::components::{
            pb_trigger_area_result::Trigger, ColliderLayer, PbTriggerArea, PbTriggerAreaResult,
            TriggerAreaEventType, TriggerAreaMeshType,
        },
    },
    SceneComponentId, SceneEntityId,
};

use crate::{
    renderer_context::RendererSceneContext,
    update_scene::pointer_results::{AvatarColliders, PointerRay},
    update_world::{
        mesh_collider::{
            add_collider_systems, ColliderId, ColliderType, HasCollider, MeshCollider,
            MeshColliderShape, SceneColliderData,
        },
        AddCrdtInterfaceExt,
    },
    ContainerEntity,
};

#[derive(Clone)]
pub struct CtTrigger;
impl ColliderType for CtTrigger {
    fn is_trigger() -> bool {
        true
    }

    fn primitive_debug_color() -> Color {
        Color::srgba(1.0, 0.0, 1.0, 0.2)
    }

    fn gltf_debug_color() -> Color {
        Color::srgba(0.0, 1.0, 1.0, 0.2)
    }
}

impl Default for MeshCollider<CtTrigger> {
    fn default() -> Self {
        Self {
            shape: MeshColliderShape::Box,
            collision_mask: ColliderLayer::ClPlayer as u32,
            mesh_name: Default::default(),
            index: Default::default(),
            _p: Default::default(),
        }
    }
}

impl From<PbTriggerArea> for MeshCollider<CtTrigger> {
    fn from(value: PbTriggerArea) -> Self {
        let shape = match value.mesh() {
            TriggerAreaMeshType::TamtBox => MeshColliderShape::Box,
            TriggerAreaMeshType::TamtSphere => MeshColliderShape::Sphere,
        };

        Self {
            shape,
            collision_mask: value
                .collision_mask
                .unwrap_or(ColliderLayer::ClPlayer as u32),
            ..Default::default()
        }
    }
}

pub struct TriggerAreaPlugin;

impl Plugin for TriggerAreaPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MonotonicTimestamp<PbTriggerAreaResult>>();

        app.add_crdt_lww_component::<PbTriggerArea, MeshCollider<CtTrigger>>(
            SceneComponentId::TRIGGER_AREA,
            ComponentPosition::EntityOnly,
        );

        add_collider_systems::<CtTrigger>(app);

        app.add_systems(Update, (update_triggers).chain().in_set(SceneSets::Input));
    }
}

#[derive(Component)]
pub struct ActiveTriggers {
    pub scene: HashMap<ColliderId, u32>,
    pub avatars: HashMap<ColliderId, u32>,
    pub pointer: HashMap<ColliderId, u32>,
}

fn update_triggers(
    mut commands: Commands,
    trigger_areas: Query<(
        Entity,
        &ContainerEntity,
        Ref<MeshCollider<CtTrigger>>,
        &HasCollider<CtTrigger>,
        &GlobalTransform,
        Option<&ActiveTriggers>,
    )>,
    mut scenes: Query<(&mut RendererSceneContext, &mut SceneColliderData)>,
    mut avatar_colliders: ResMut<AvatarColliders>,
    triggers: Query<(&MeshCollider<CtTrigger>, &GlobalTransform)>,
    frame: Res<FrameCount>,
    pointer_ray: Res<PointerRay>,
    timestamp: Res<MonotonicTimestamp<PbTriggerAreaResult>>,
) {
    let make_trigger = |colliders: &SceneColliderData, collider_id: &ColliderId| -> Trigger {
        colliders
            .get_collider_entity(collider_id)
            .and_then(|e| triggers.get(e).ok())
            .map(|(collider, gt)| {
                let (s, r, t) = gt.to_scale_rotation_translation();
                Trigger {
                    entity: collider_id.entity.as_proto_u32().unwrap(),
                    layers: collider.collision_mask,
                    position: Some(Vector3::world_vec_from_vec3(&t)),
                    rotation: Some(r.into()),
                    scale: Some(Vector3::abs_vec_from_vec3(&s)),
                }
            })
            .unwrap_or_else(|| Trigger {
                entity: collider_id.entity.as_proto_u32().unwrap(),
                layers: 0,
                position: None,
                rotation: None,
                scale: None,
            })
    };

    let make_result = |colliders: &SceneColliderData,
                       container: &ContainerEntity,
                       translation: Vec3,
                       rotation: Quat,
                       timestamp: &MonotonicTimestamp<PbTriggerAreaResult>,
                       collider_id: &ColliderId,
                       ty: TriggerAreaEventType|
     -> PbTriggerAreaResult {
        PbTriggerAreaResult {
            triggered_entity: container.container_id.as_proto_u32().unwrap(),
            triggered_entity_position: Some(Vector3::world_vec_from_vec3(&translation)),
            triggered_entity_rotation: Some(rotation.into()),
            event_type: ty as i32,
            timestamp: timestamp.next_timestamp(),
            trigger: Some(make_trigger(colliders, collider_id)),
        }
    };

    let make_events = |colliders: &SceneColliderData,
                       active_colliders: &HashMap<ColliderId, u32>,
                       new_colliders: &Vec<ColliderId>,
                       container: &ContainerEntity,
                       translation: Vec3,
                       rotation: Quat,
                       tick: u32,
                       timestamp: &MonotonicTimestamp<PbTriggerAreaResult>|
     -> Vec<(SceneEntityId, PbTriggerAreaResult)> {
        let mut results = Vec::default();

        for (prev_collider, prev_frame) in active_colliders {
            if new_colliders.contains(prev_collider) {
                if prev_frame != &tick {
                    // send only 1 stay per scene tick
                    results.push((
                        container.container_id,
                        make_result(
                            colliders,
                            container,
                            translation,
                            rotation,
                            timestamp,
                            prev_collider,
                            TriggerAreaEventType::TaetStay,
                        ),
                    ));
                }
            } else {
                results.push((
                    container.container_id,
                    make_result(
                        colliders,
                        container,
                        translation,
                        rotation,
                        timestamp,
                        prev_collider,
                        TriggerAreaEventType::TaetExit,
                    ),
                ));
            }
        }

        for new_collider in new_colliders {
            if !active_colliders.contains_key(new_collider) {
                results.push((
                    container.container_id,
                    make_result(
                        colliders,
                        container,
                        translation,
                        rotation,
                        timestamp,
                        new_collider,
                        TriggerAreaEventType::TaetEnter,
                    ),
                ));
            }
        }

        results
    };

    for (entity, container, trigger_def, collider, gt, maybe_active) in trigger_areas.iter() {
        let Ok((mut scene, mut colliders)) = scenes.get_mut(container.root) else {
            continue;
        };

        let (_, rotation, translation) = gt.to_scale_rotation_translation();

        // get intersecting colliders
        let new_colliders = colliders.intersect_id(
            scene.last_update_frame,
            &collider.0,
            trigger_def.collision_mask,
        );

        let empty_active = HashMap::default();
        let mut results = make_events(
            &colliders,
            maybe_active
                .as_ref()
                .map(|a| &a.scene)
                .unwrap_or(&empty_active),
            &new_colliders,
            container,
            translation,
            rotation,
            scene.last_update_frame,
            &timestamp,
        );

        // get avatar colliders
        let mut new_avatars = Default::default();
        if trigger_def.collision_mask & (ColliderLayer::ClPlayer as u32) != 0 {
            new_avatars = colliders
                .get_collider(&collider.0)
                .map(|c| {
                    avatar_colliders.collider_data.intersect_collider(
                        frame.0,
                        c,
                        trigger_def.collision_mask,
                    )
                })
                .unwrap_or_default();
            results.extend(make_events(
                &avatar_colliders.collider_data,
                maybe_active
                    .as_ref()
                    .map(|a| &a.avatars)
                    .unwrap_or(&empty_active),
                &new_avatars,
                container,
                translation,
                rotation,
                scene.last_update_frame,
                &timestamp,
            ));
        } else {
            Default::default()
        }

        // get pointer ray
        let mut new_pointers = Default::default();
        if trigger_def.collision_mask & (ColliderLayer::ClPointer as u32) != 0 {
            if let Some(ray) = pointer_ray.0 {
                let pointer_hit = colliders.cast_ray_nearest(
                    scene.last_update_frame,
                    ray.origin,
                    ray.direction.as_vec3(),
                    f32::MAX,
                    ColliderLayer::ClPointer as u32,
                    false,
                    true,
                    Some(&collider.0),
                );

                new_pointers = if pointer_hit.is_some() {
                    vec![ColliderId::default()]
                } else {
                    Vec::default()
                };

                results.extend(make_events(
                    &colliders,
                    maybe_active
                        .as_ref()
                        .map(|a| &a.pointer)
                        .unwrap_or(&empty_active),
                    &new_pointers,
                    container,
                    translation,
                    rotation,
                    scene.last_update_frame,
                    &timestamp,
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

        commands.entity(entity).try_insert(ActiveTriggers {
            scene: new_colliders
                .into_iter()
                .map(|c| (c, scene.last_update_frame))
                .collect(),
            avatars: new_avatars
                .into_iter()
                .map(|c| (c, scene.last_update_frame))
                .collect(),
            pointer: new_pointers
                .into_iter()
                .map(|c| (c, scene.last_update_frame))
                .collect(),
        });
    }
}
