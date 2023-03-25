use bevy::prelude::*;

use crate::{
    dcl::interface::ComponentPosition,
    dcl_component::{
        proto_components::sdk::components::{pb_raycast::Direction, PbRaycast},
        SceneComponentId,
    },
    scene_runner::{RendererSceneContext, SceneEntity, SceneSets},
};

use super::{mesh_collider::SceneColliderData, AddCrdtInterfaceExt};

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
    mut scene_datas: Query<(&RendererSceneContext, &mut SceneColliderData)>,
) {
    for (e, scene_ent, ray, transform) in raycast_requests.iter() {
        debug!("{e:?} has raycast request: {ray:?}");
        if let Ok((context, mut scene_data)) = scene_datas.get_mut(scene_ent.root) {
            let origin = transform.translation()
                + ray
                    .0
                    .origin_offset
                    .as_ref()
                    .map(<Vec3 as From<_>>::from)
                    .unwrap_or(Vec3::ZERO);
            let direction = match &ray.0.direction {
                Some(Direction::LocalDirection(dir)) => transform.transform_point(dir.into()),
                Some(Direction::GlobalDirection(dir)) => dir.into(),
                Some(Direction::GlobalTarget(point)) => Vec3::from(point) - origin,
                Some(Direction::TargetEntity(_id)) => todo!(),
                None => {
                    warn!("no direction on raycast");
                    continue;
                }
            };
            let result =
                scene_data.cast_ray_nearest(context.last_sent, origin, direction, f32::MAX);
            if let Some(result) = result {
                info!("{} raycast hit: {result}", scene_ent.id);
            }
        }
    }
}
