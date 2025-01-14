// TODO
// [x] - handle continuous properly
// [x] - don't run every renderer frame
// [/] - then prevent scene execution until raycasts are run (not required now, we run raycasts once on first frame after request arrives, required for ponter events anyway)
// [x] - probably change renderer context to contain frame number as well as dt so we can track precisely track run state
// [ ] - move into scene loop
// [/] - consider how global raycasts interact with this setup (it works, pointer events use a global raycast already. but need to optimise by ordering scenes based on ray)

use bevy::{color::palettes::basic, prelude::*};
use bevy_console::ConsoleCommand;

use crate::{
    update_world::{
        gltf_container::GLTF_LOADING,
        mesh_collider::{RaycastResult, SceneColliderData},
        raycast::Raycast,
    },
    ContainingScene, RendererSceneContext, SceneEntity, SceneSets,
};
use console::DoAddConsoleCommand;
use dcl::interface::CrdtType;
use dcl_component::{
    proto_components::{
        common::Vector3,
        sdk::components::{
            common::RaycastHit, pb_raycast::Direction, ColliderLayer, PbRaycastResult,
            RaycastQueryType,
        },
        RoughRoundExt,
    },
    SceneComponentId, SceneEntityId,
};

pub struct RaycastResultPlugin;

impl Plugin for RaycastResultPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, run_raycasts.in_set(SceneSets::Input));
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

#[allow(clippy::too_many_arguments)]
fn run_raycasts(
    mut raycast_requests: Query<(Entity, &SceneEntity, &mut Raycast, &GlobalTransform)>,
    target_positions: Query<&GlobalTransform>,
    mut scene_context: Query<(
        Entity,
        &mut RendererSceneContext,
        &mut SceneColliderData,
        &GlobalTransform,
    )>,
    debug: Res<DebugRaycast>,
    mut gizmos: Gizmos,
    time: Res<Time>,
    mut gizmo_cache: Local<Vec<(f32, Vec3, Vec3)>>,
    containing_scene: ContainingScene,
) {
    // redraw non-continuous gizmos for 1 sec
    gizmo_cache.retain(|(until, origin, end)| {
        gizmos.line(*origin, *end, basic::BLUE);
        time.elapsed_seconds() > *until
    });

    for (e, scene_ent, mut raycast, transform) in raycast_requests.iter_mut() {
        debug!("{e:?} has raycast request: {raycast:?}");
        let Ok((_, context, mut scene_data, scene_transform)) =
            scene_context.get_mut(scene_ent.root)
        else {
            continue;
        };

        // check if we can run
        if context.blocked.contains(GLTF_LOADING) {
            debug!("raycast skipped, waiting for gltfs");
            continue;
        }

        // check if we need to run
        let continuous = raycast.raycast.continuous.unwrap_or(false);
        if !continuous && raycast.last_run > 0 {
            continue;
        }
        if continuous && raycast.last_run >= context.last_update_frame {
            continue;
        }
        raycast.last_run = context.last_update_frame;
        debug!("running raycast");

        // execute the raycast
        let raycast = &raycast.raycast;

        let (_, local_rotation, _) = transform.to_scale_rotation_translation();
        let scene_translation = scene_transform.translation();

        let offset = raycast
            .origin_offset
            .as_ref()
            .map(Vector3::world_vec_to_vec3)
            .unwrap_or(Vec3::ZERO);
        let origin = transform.transform_point(offset);
        let direction = match &raycast.direction {
            Some(Direction::LocalDirection(dir)) => local_rotation * dir.world_vec_to_vec3(),
            Some(Direction::GlobalDirection(dir)) => dir.world_vec_to_vec3(),
            Some(Direction::GlobalTarget(point)) => {
                point.world_vec_to_vec3() + scene_translation - origin
            }
            Some(Direction::TargetEntity(id)) => {
                let target_position = context
                    .bevy_entity(SceneEntityId::from_proto_u32(*id))
                    .and_then(|entity| target_positions.get(entity).ok())
                    .map(|gt| gt.translation())
                    .unwrap_or(origin);
                target_position - origin
            }
            None => {
                warn!("no direction on raycast");
                continue;
            }
        }
        .normalize();

        let mask = raycast
            .collision_mask
            .unwrap_or(ColliderLayer::ClPointer as u32 | ColliderLayer::ClPhysics as u32);

        let results = match (context.is_portable, raycast.query_type()) {
            (false, RaycastQueryType::RqtHitFirst) => scene_data
                .cast_ray_nearest(
                    context.last_update_frame,
                    origin,
                    direction,
                    raycast.max_distance,
                    mask,
                    true,
                )
                .map(|hit| vec![(scene_ent.root, hit)])
                .unwrap_or_default(),
            (false, RaycastQueryType::RqtQueryAll) => scene_data
                .cast_ray_all(
                    context.last_update_frame,
                    origin,
                    direction,
                    raycast.max_distance,
                    mask,
                    true,
                )
                .into_iter()
                .map(|hit| (scene_ent.root, hit))
                .collect(),
            (true, RaycastQueryType::RqtHitFirst) => {
                let full_ray = direction * raycast.max_distance;
                let mut scenes = containing_scene
                    .get_ray(origin, full_ray)
                    .into_iter()
                    .peekable();
                let mut best_result: Option<(Entity, RaycastResult)> = None;
                while scenes.peek().is_some_and(|(_, closest)| {
                    best_result
                        .as_ref()
                        .map_or(true, |(_, br)| br.toi > *closest)
                }) {
                    let scene = scenes.next().unwrap().0;
                    let Ok((scene, context, mut colliders, _)) = scene_context.get_mut(scene)
                    else {
                        continue;
                    };
                    if let Some(result) = colliders.cast_ray_nearest(
                        context.last_update_frame,
                        origin,
                        direction,
                        raycast.max_distance,
                        mask,
                        true,
                    ) {
                        if best_result
                            .as_ref()
                            .map_or(true, |(_, b)| b.toi > result.toi)
                        {
                            best_result = Some((scene, result));
                        }
                    }
                }

                if let Some(result) = best_result {
                    vec![result]
                } else {
                    Vec::default()
                }
            }
            (true, RaycastQueryType::RqtQueryAll) => {
                let full_ray = direction * raycast.max_distance;
                let mut results = Vec::new();
                for (scene, _) in containing_scene.get_ray(origin, full_ray) {
                    let Ok((scene, context, mut colliders, _)) = scene_context.get_mut(scene)
                    else {
                        continue;
                    };
                    for result in colliders
                        .cast_ray_all(
                            context.last_update_frame,
                            origin,
                            direction,
                            raycast.max_distance,
                            mask,
                            true,
                        )
                        .into_iter()
                    {
                        results.push((scene, result));
                    }
                }

                results
            }
            (_, RaycastQueryType::RqtNone) => Vec::default(),
        };

        // debug line showing raycast
        if debug.0 {
            let end = origin + direction * raycast.max_distance;
            gizmos.line(origin, end, basic::BLUE);
            if !continuous {
                gizmo_cache.push((time.elapsed_seconds() + 1.0, origin, end));
            }
        }

        // output
        let scene_origin = origin - scene_translation;

        let make_hit = |(scene, result): (Entity, RaycastResult)| -> RaycastHit {
            RaycastHit {
                position: Some(Vector3::world_vec_from_vec3(
                    &(scene_origin + direction * result.toi).round_at_pow2(-14),
                )),
                global_origin: Some(Vector3::world_vec_from_vec3(
                    &scene_origin.round_at_pow2(-14),
                )),
                direction: Some(Vector3::world_vec_from_vec3(&direction.round_at_pow2(-14))),
                normal_hit: Some(Vector3::world_vec_from_vec3(
                    &result.normal.round_at_pow2(-14),
                )),
                length: result.toi,
                // only pass details for hits on current scene entities
                mesh_name: if scene == scene_ent.root {
                    result.id.name
                } else {
                    None
                },
                entity_id: if scene == scene_ent.root {
                    result.id.entity.as_proto_u32()
                } else {
                    None
                },
            }
        };

        // lookup again as global raycasts require access to other contexts
        let mut context = scene_context.get_mut(scene_ent.root).unwrap().1;

        let result = PbRaycastResult {
            timestamp: raycast.timestamp,
            global_origin: Some(Vector3::world_vec_from_vec3(&scene_origin)),
            direction: Some(Vector3::world_vec_from_vec3(&direction.round_at_pow2(-14))),
            hits: results.into_iter().map(make_hit).collect(),
            tick_number: context.tick_number,
        };

        context.update_crdt(
            SceneComponentId::RAYCAST_RESULT,
            CrdtType::LWW_ENT,
            scene_ent.id,
            &result,
        );
    }
}
