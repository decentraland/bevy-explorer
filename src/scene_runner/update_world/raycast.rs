use bevy::prelude::*;
use bevy_prototype_debug_lines::DebugLines;

use crate::{
    dcl::interface::{ComponentPosition, CrdtType},
    dcl_component::{
        proto_components::{sdk::components::{
            common::RaycastHit, pb_raycast::Direction, PbRaycast, PbRaycastResult, RaycastQueryType,
        }, common::Vector3},
        SceneComponentId,
    },
    scene_runner::{RendererSceneContext, SceneEntity, SceneSets},
};

use super::{
    mesh_collider::{RaycastResult, SceneColliderData},
    AddCrdtInterfaceExt,
};

pub struct RaycastPlugin;

#[derive(Component, Debug)]
pub struct Raycast(PbRaycast);

impl From<PbRaycast> for Raycast {
    fn from(value: PbRaycast) -> Self {
        Self(value)
    }
}

impl Plugin for RaycastPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbRaycast, Raycast>(
            SceneComponentId::RAYCAST,
            ComponentPosition::EntityOnly,
        );
        app.add_system(run_raycasts.in_set(SceneSets::Input));
    }
}

fn run_raycasts(
    raycast_requests: Query<(Entity, &SceneEntity, &Raycast, &GlobalTransform)>,
    _target_positions: Query<(Entity, &GlobalTransform)>,
    mut scene_datas: Query<(
        &mut RendererSceneContext,
        &mut SceneColliderData,
        &GlobalTransform,
    )>,
    mut lines: ResMut<DebugLines>,
) {
    for (e, scene_ent, raycast, transform) in raycast_requests.iter() {
        debug!("{e:?} has raycast request: {raycast:?}");
        if let Ok((mut context, mut scene_data, scene_transform)) =
            scene_datas.get_mut(scene_ent.root)
        {
            let (_, local_rotation, _) = transform.to_scale_rotation_translation();
            let scene_translation = scene_transform.translation();
            let mut db = false;

            let offset = raycast
                .0
                .origin_offset
                .as_ref()
                .map(Vector3::world_vec_to_vec3)
                .unwrap_or(Vec3::ZERO);
            let origin = transform.transform_point(offset);
            let direction = match &raycast.0.direction {
                Some(Direction::LocalDirection(dir)) => local_rotation * dir.world_vec_to_vec3(),
                Some(Direction::GlobalDirection(dir)) => {
                    db = true;
                    dir.world_vec_to_vec3()
                }
                Some(Direction::GlobalTarget(point)) => point.world_vec_to_vec3() + scene_translation - origin,
                Some(Direction::TargetEntity(_id)) => todo!(),
                None => {
                    warn!("no direction on raycast");
                    continue;
                }
            }.normalize();
            let results = match raycast.0.query_type() {
                RaycastQueryType::RqtHitFirst => scene_data
                    .cast_ray_nearest(context.last_sent, origin, direction, f32::MAX, raycast.0.collision_mask.unwrap_or(u32::MAX))
                    .map(|hit| vec![hit])
                    .unwrap_or_default(),
                RaycastQueryType::RqtQueryAll => {
                    scene_data.cast_ray_all(context.last_sent, origin, direction, raycast.0.max_distance, raycast.0.collision_mask.unwrap_or(u32::MAX))
                }
                RaycastQueryType::RqtNone => Vec::default(),
            };

            if db {
                debug!("{}: origin: {origin}, direction: {direction}, hits: {results:?}", scene_ent.id);
            }

            lines.line_colored(origin, origin + direction * 100.0, 0.0, Color::BLUE);

            // output
            let scene_origin = origin - scene_translation;

            let make_hit = |result: RaycastResult| -> RaycastHit {
                RaycastHit {
                    position: Some(Vector3::world_vec_from_vec3(&(scene_origin + direction * result.toi))),
                    global_origin: Some(Vector3::world_vec_from_vec3(&scene_origin)),
                    direction: Some(Vector3::world_vec_from_vec3(&direction)),
                    normal_hit: Some(Vector3::world_vec_from_vec3(&result.normal)),
                    length: result.toi,
                    mesh_name: Default::default(),
                    entity_id: result.id.as_proto_u32(),
                }
            };

            let result = PbRaycastResult {
                timestamp: raycast.0.timestamp,
                global_origin: Some(Vector3::world_vec_from_vec3(&scene_origin)),
                direction: Some(Vector3::world_vec_from_vec3(&direction)),
                hits: results.into_iter().map(make_hit).collect(),
            };

            context.update_crdt(
                SceneComponentId::RAYCAST_RESULT,
                CrdtType::LWW_ENT,
                scene_ent.id,
                &result,
            );
        }
    }
}
