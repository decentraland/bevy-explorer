// TODO
// - handle continuous properly
// - don't run every frame
// - then prevent scene execution until raycasts are run
// - probably change renderer context to contain frame number as well as dt so we can track precisely track run state
// - move into scene loop
// - consider how global raycasts interact with this setup

use bevy::prelude::*;
use bevy_console::ConsoleCommand;
#[cfg(not(test))]
use bevy_prototype_debug_lines::DebugLines;

use crate::{
    console::DoAddConsoleCommand,
    dcl::interface::{ComponentPosition, CrdtType},
    dcl_component::{
        proto_components::{
            common::Vector3,
            sdk::components::{
                common::RaycastHit, pb_raycast::Direction, ColliderLayer, PbRaycast,
                PbRaycastResult, RaycastQueryType,
            },
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
        app.init_resource::<DebugRaycast>();
        app.add_console_command::<DebugRaycastCommand, _>(debug_raycast);
    }
}

#[derive(Resource, Default)]
struct DebugRaycast(bool);

/// Toggle debug lines for raycasts
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/debug_raycast")]
struct DebugRaycastCommand {
    show: Option<bool>,
}

fn debug_raycast(mut input: ConsoleCommand<DebugRaycastCommand>, mut debug: ResMut<DebugRaycast>) {
    if let Some(Ok(command)) = input.take() {
        let new_state = command.show.unwrap_or(!debug.0);
        debug.0 = new_state;
        input.reply_ok(format!("showing debug raycast lines: {new_state}"));
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
    #[cfg(not(test))] mut lines: ResMut<DebugLines>,
    debug: Res<DebugRaycast>,
) {
    for (e, scene_ent, raycast, transform) in raycast_requests.iter() {
        debug!("{e:?} has raycast request: {raycast:?}");
        if let Ok((mut context, mut scene_data, scene_transform)) =
            scene_datas.get_mut(scene_ent.root)
        {
            let (_, local_rotation, _) = transform.to_scale_rotation_translation();
            let scene_translation = scene_transform.translation();

            let offset = raycast
                .0
                .origin_offset
                .as_ref()
                .map(Vector3::world_vec_to_vec3)
                .unwrap_or(Vec3::ZERO);
            let origin = transform.transform_point(offset);
            let direction = match &raycast.0.direction {
                Some(Direction::LocalDirection(dir)) => local_rotation * dir.world_vec_to_vec3(),
                Some(Direction::GlobalDirection(dir)) => dir.world_vec_to_vec3(),
                Some(Direction::GlobalTarget(point)) => {
                    point.world_vec_to_vec3() + scene_translation - origin
                }
                Some(Direction::TargetEntity(_id)) => todo!(),
                None => {
                    warn!("no direction on raycast");
                    continue;
                }
            }
            .normalize();

            let results = match raycast.0.query_type() {
                RaycastQueryType::RqtHitFirst => scene_data
                    .cast_ray_nearest(
                        context.last_sent,
                        origin,
                        direction,
                        raycast.0.max_distance,
                        raycast.0.collision_mask.unwrap_or(
                            ColliderLayer::ClPointer as u32 | ColliderLayer::ClPhysics as u32,
                        ),
                    )
                    .map(|hit| vec![hit])
                    .unwrap_or_default(),
                RaycastQueryType::RqtQueryAll => scene_data.cast_ray_all(
                    context.last_sent,
                    origin,
                    direction,
                    raycast.0.max_distance,
                    raycast.0.collision_mask.unwrap_or(
                        ColliderLayer::ClPointer as u32 | ColliderLayer::ClPhysics as u32,
                    ),
                ),
                RaycastQueryType::RqtNone => Vec::default(),
            };

            // debug line showing raycast
            if debug.0 {
                #[cfg(not(test))]
                lines.line_colored(
                    origin,
                    origin + direction * raycast.0.max_distance,
                    0.0,
                    Color::BLUE,
                );
            }

            // output
            let scene_origin = origin - scene_translation;

            let make_hit = |result: RaycastResult| -> RaycastHit {
                RaycastHit {
                    position: Some(Vector3::world_vec_from_vec3(
                        &(scene_origin + direction * result.toi),
                    )),
                    global_origin: Some(Vector3::world_vec_from_vec3(&scene_origin)),
                    direction: Some(Vector3::world_vec_from_vec3(&direction)),
                    normal_hit: Some(Vector3::world_vec_from_vec3(&result.normal)),
                    length: result.toi,
                    mesh_name: result.id.name,
                    entity_id: result.id.entity.as_proto_u32(),
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
