use bevy::prelude::*;

use crate::{
    dcl::interface::{ComponentPosition, CrdtType},
    dcl_component::{
        proto_components::sdk::components::{
            common::RaycastHit, pb_raycast::Direction, PbRaycast, PbRaycastResult, RaycastQueryType,
        },
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
) {
    for (e, scene_ent, raycast, transform) in raycast_requests.iter() {
        debug!("{e:?} has raycast request: {raycast:?}");
        if let Ok((mut context, mut scene_data, scene_transform)) =
            scene_datas.get_mut(scene_ent.root)
        {
            let offset = raycast
                .0
                .origin_offset
                .as_ref()
                .copied()
                .map(<Vec3 as From<_>>::from)
                .unwrap_or(Vec3::ZERO);
            let origin = transform.translation() + offset;
            let direction = match &raycast.0.direction {
                Some(Direction::LocalDirection(dir)) => transform.transform_point(Vec3::from(*dir)),
                Some(Direction::GlobalDirection(dir)) => Vec3::from(*dir),
                Some(Direction::GlobalTarget(point)) => Vec3::from(*point) - origin,
                Some(Direction::TargetEntity(_id)) => todo!(),
                None => {
                    warn!("no direction on raycast");
                    continue;
                }
            };
            let results = match raycast.0.query_type() {
                RaycastQueryType::RqtHitFirst => scene_data
                    .cast_ray_nearest(context.last_sent, origin, direction, f32::MAX)
                    .map(|hit| vec![hit])
                    .unwrap_or_default(),
                RaycastQueryType::RqtQueryAll => {
                    scene_data.cast_ray_all(context.last_sent, origin, direction, f32::MAX)
                }
                RaycastQueryType::RqtNone => Vec::default(),
            };

            info!("{} raycast hits: {results:?}", scene_ent.id);

            // output
            let scene_translation = scene_transform.translation();
            let global_origin = transform.translation() - scene_translation + offset;

            let make_hit = |result: RaycastResult| -> RaycastHit {
                RaycastHit {
                    position: Some((global_origin + direction * result.toi).into()),
                    global_origin: Some(global_origin.into()),
                    direction: Some(direction.into()),
                    normal_hit: Some(result.normal.into()),
                    length: result.toi,
                    mesh_name: Default::default(),
                    entity_id: result.id.as_proto_u32(),
                }
            };

            let result = PbRaycastResult {
                timestamp: raycast.0.timestamp,
                global_origin: Some(global_origin.into()),
                direction: Some(direction.into()),
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
