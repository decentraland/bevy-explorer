// ensuring that player moves with a ground collider (platform) is tricky. to ensure consistency we consider the process starting from the scene loop :

// * collider gts = entity gts = entity transforms
// * player gt = player transform / no penetration with colliders

// scene loop
// - update entity transforms (so entity transform != global transform != collider transform)
// - leave collider transforms in old state

// dynamics - setup
// - use last ground-collider transform
// - manually calculate entity global transform
// - record difference
// - apply difference to ground-collider collider transform, ignoring player (so ground-collider entity transform == collider transform)

// dynamics - update player
// - in: player intrinsic velocity
// - in: ground-collider difference, (we don't currently but could clamp to max fall speed (based on scene elapsed for last tick? absolute? both?))
// - clamp player intrinsic velocity by ground-collider (input velocity should still be constrained by what you are standing on)
// - clamp (player intrinsic velocity + gc difference) by all colliders except ground-collider
// - use clamped (motion + difference) to update position and intrinsic velocity

// postupdate

// - [bevy] animations
// - [gltf_container] sync gltf nodes
// ** all collidable (non-attached items) are in final positions
// - [here] dynamics - update player
// - [transform_and_parent] - update ParentPositionSync<AvatarAttachStage>
// - [user_input] - camera position
// - [transform_and_parent] - update ParentPositionSync<SceneProxyStage>
// - update global transforms
// - render (player position is updated for gc/platform only, all collider entity global transforms are updated to their new positions; collider transforms are not but this doesn't affect rendering)
// note player may intersect non-ground colliders (or weirdly rotating gcs), just for rendering

// postinit
// - update collider transforms (ground-collider won't change), checking for push/pen on player
// - ground-collider can't push player as transform is up to date already
// - ground-collider (and others) depenetrate player if intersecting

// * collider gts = entity gts = entity transforms
// * player gt = player transform / no penetration with colliders

// and back to scene loop

use bevy::{
    core::FrameCount,
    math::{DVec3, Vec3Swizzles},
    prelude::*,
};
use bevy_console::ConsoleCommand;
use rapier3d_f64::control::{CharacterAutostep, CharacterLength, KinematicCharacterController};

use common::{
    dynamics::{
        MAX_CLIMBABLE_INCLINE, MAX_STEP_HEIGHT, PLAYER_COLLIDER_OVERLAP, PLAYER_COLLIDER_RADIUS,
        PLAYER_GROUND_THRESHOLD,
    },
    structs::{AvatarDynamicState, PlayerModifiers, PrimaryUser},
};

use scene_runner::{
    renderer_context::RendererSceneContext,
    update_world::mesh_collider::{ColliderId, GroundCollider, SceneColliderData},
    ContainingScene, OutOfWorld,
};

#[derive(Resource)]
pub struct UserClipping(pub bool);

const TICK_TIME: f32 = 1.0 / 720.0;

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn update_user_position(
    mut player: Query<
        (
            Entity,
            &PrimaryUser,
            Option<&PlayerModifiers>,
            &mut Transform,
            &mut AvatarDynamicState,
            &mut GroundCollider,
        ),
        Without<OutOfWorld>,
    >,
    mut scene_datas: Query<(&mut RendererSceneContext, &mut SceneColliderData)>,
    containing_scenes: ContainingScene,
    time: Res<Time>,
    _frame: Res<FrameCount>,
    manual_transform: Query<(&Transform, Option<&Parent>), Without<PrimaryUser>>,
    clip: Res<UserClipping>,
    mut prev_excess_time: Local<f32>,
) {
    let Ok((
        user_ent,
        user,
        maybe_modifiers,
        mut transform,
        mut dynamic_state,
        mut ground_collider,
    )) = player.get_single_mut()
    else {
        return;
    };

    let user = maybe_modifiers
        .map(|m| m.combine(user))
        .unwrap_or_else(|| user.clone());

    let dt = time.delta_secs();

    let mut velocity = dynamic_state.velocity;
    let force = (dynamic_state.force * user.friction).extend(0.0).xzy();

    let mut elapsed = time.delta_secs() + *prev_excess_time;
    let mut target_motion = Vec3::ZERO;

    let record_peak = velocity.y > 0.0;

    while elapsed > TICK_TIME {
        velocity.x = (velocity.x + force.x * TICK_TIME) * (-user.friction * TICK_TIME).exp();
        velocity.z = (velocity.z + force.z * TICK_TIME) * (-user.friction * TICK_TIME).exp();
        // no friction for y
        velocity.y += user.gravity * TICK_TIME;

        target_motion.x += velocity.x * TICK_TIME;
        target_motion.z += velocity.z * TICK_TIME;
        target_motion.y += if record_peak {
            velocity.y.max(0.0) * TICK_TIME
        } else {
            velocity.y * TICK_TIME
        };

        elapsed -= TICK_TIME;
    }
    *prev_excess_time = elapsed;

    dynamic_state.velocity = velocity;

    if dynamic_state.tank {
        // rotate as instructed
        transform.rotation *= Quat::from_rotation_y(dynamic_state.rotate * time.delta_secs());
    } else {
        // rotate towards velocity vec
        if dynamic_state.force.length() > 0.0 {
            let target_rotation = Transform::default()
                .looking_at(dynamic_state.force.extend(0.0).xzy(), Vec3::Y)
                .rotation;

            transform.rotation = transform.rotation.lerp(target_rotation, dt * 10.0);
        }
    }

    let mut platform_motion = Vec3::default();
    let mut platform_handle: Option<(Entity, ColliderId)> = None;
    dynamic_state.ground_height = transform.translation.y;

    let calc_global_transform = |entity: Entity| -> GlobalTransform {
        let Ok((sync_transform, maybe_parent)) = manual_transform.get(entity) else {
            return GlobalTransform::default();
        };

        let mut transforms = vec![sync_transform];
        let mut pointer = maybe_parent;
        while let Some(next_parent) = pointer {
            let Ok((next_transform, next_parent)) = manual_transform.get(next_parent.get()) else {
                break;
            };

            transforms.push(next_transform);
            pointer = next_parent;
        }

        let mut new_global_transform = GlobalTransform::default();
        while let Some(next_transform) = transforms.pop() {
            new_global_transform = new_global_transform.mul_transform(*next_transform);
        }
        new_global_transform
    };

    // setup physics controller
    let mut controller = KinematicCharacterController {
        offset: CharacterLength::Absolute(PLAYER_COLLIDER_OVERLAP as f64),
        slide: true,
        autostep: Some(CharacterAutostep {
            max_height: CharacterLength::Absolute(MAX_STEP_HEIGHT as f64),
            min_width: CharacterLength::Relative(0.75),
            include_dynamic_bodies: true,
        }),
        max_slope_climb_angle: MAX_CLIMBABLE_INCLINE as f64,
        min_slope_slide_angle: MAX_CLIMBABLE_INCLINE as f64,
        snap_to_ground: Some(CharacterLength::Absolute(0.1)),
        ..Default::default()
    };
    // unset autostep when jumping
    if dynamic_state.velocity.y > 0.0 {
        controller.autostep = None;
    }

    // adjust by ground-collider's movement
    if let Some((scene_ent, collider, old_transform)) = ground_collider.0.take() {
        if let Ok((ctx, mut collider_data)) = scene_datas.get_mut(scene_ent) {
            let new_global_transform = collider_data
                .get_collider_entity(&collider)
                .map(calc_global_transform)
                .unwrap_or(old_transform);

            if new_global_transform != old_transform {
                // update rotation
                let rotation_change = new_global_transform.to_scale_rotation_translation().1
                    * old_transform.to_scale_rotation_translation().1.inverse();
                // clamp to x/z plane to avoid twisting around
                let new_facing = ((rotation_change * Vec3::from(transform.forward()))
                    * (Vec3::X + Vec3::Z))
                    .normalize();
                transform.look_to(new_facing, Vec3::Y);
                // and rotate velocity
                dynamic_state.velocity = rotation_change * dynamic_state.velocity;

                // calculate updated translation and add to motion
                let player_global_transform = GlobalTransform::from(*transform);
                let relative_position = player_global_transform.reparented_to(&old_transform);
                let new_transform = new_global_transform.mul_transform(relative_position);
                let new_translation = new_transform.translation();
                platform_motion = new_translation - transform.translation;

                if target_motion.y > 0.0 && platform_motion.y / dt > target_motion.y {
                    // special case jumping on a platform moving up
                    // dynamic_state.velocity.y = 0.0;//(platform_motion.y / dt).max(0.0);
                    target_motion.y = 0.0;
                    debug!("cap jump");
                }

                // force update new collider
                let prior_cpos = DVec3::from(
                    collider_data
                        .get_collider(&collider)
                        .unwrap()
                        .position()
                        .translation,
                )
                .as_vec3();
                collider_data.update_collider_transform(&collider, &new_global_transform, None);
                let new_cpos = DVec3::from(
                    collider_data
                        .get_collider(&collider)
                        .unwrap()
                        .position()
                        .translation,
                )
                .as_vec3();

                // adjust base motion wrt ground collider
                if clip.0 {
                    let prior = target_motion;
                    target_motion = collider_data.move_character(
                        ctx.last_update_frame,
                        transform.translation + platform_motion,
                        target_motion,
                        &controller,
                        Some(&collider),
                        true,
                    );
                    debug!(
                        "abmwgc {} -> {} (collider {} -> {})",
                        prior.y, target_motion.y, prior_cpos, new_cpos
                    );
                }

                // add platform motion
                target_motion += platform_motion;

                platform_handle = Some((scene_ent, collider));
                debug!(
                    "platform move {} - rp {} -> {}",
                    old_transform.translation().y,
                    old_transform.translation().y - transform.translation.y,
                    new_translation.y - transform.translation.y
                );
            } else {
                debug!(
                    "platform      {} - rp {}",
                    old_transform.translation().y,
                    old_transform.translation().y - transform.translation.y
                );
            }
        }
    }

    // check containing scenes
    for scene in containing_scenes.get_area(user_ent, PLAYER_COLLIDER_RADIUS) {
        let Ok((context, mut collider_data)) = scene_datas.get_mut(scene) else {
            continue;
        };

        let platform_handle =
            platform_handle
                .as_ref()
                .and_then(|(platform_scene, platform_handle)| {
                    (platform_scene == &scene).then_some(platform_handle)
                });

        // get allowed motion for total motion wrt all but ground collider
        if clip.0 {
            target_motion = collider_data.move_character(
                context.last_update_frame,
                transform.translation,
                target_motion,
                &controller,
                platform_handle,
                false,
            );
        }
    }

    debug!(
        "dynamics: {} -> {}, (velocity = {}, platform = {})",
        transform.translation,
        transform.translation + target_motion,
        velocity,
        platform_motion
    );

    transform.translation += target_motion;
    transform.translation.y = transform.translation.y.max(0.0);
    dynamic_state.ground_height = transform.translation.y;

    // calculate ground height / ground-collider after updating
    for scene in containing_scenes.get(user_ent) {
        let Ok((context, mut collider_data)) = scene_datas.get_mut(scene) else {
            continue;
        };
        if let Some((height, collider)) =
            collider_data.get_groundheight(context.last_update_frame, transform.translation)
        {
            if height < dynamic_state.ground_height {
                dynamic_state.ground_height = height;
                if height < PLAYER_GROUND_THRESHOLD {
                    let entity = collider_data.get_collider_entity(&collider).unwrap();
                    let gt = calc_global_transform(entity);
                    ground_collider.0 = Some((scene, collider, gt));
                    debug!("still on platform (@{height})");
                } else {
                    debug!("left platform (@{height})");
                }
            }
        } else {
            debug!("no platform nearby");
        }
    }

    // update vertical velocity
    if dynamic_state.ground_height <= 0.0
        || transform.translation.y == 0.0
        || platform_motion.y != 0.0
    {
        // on the floor, set vertical velocity to zero
        dynamic_state.velocity.y = dynamic_state.velocity.y.max(0.0);
    } else if target_motion.y.abs() < (0.5 * dynamic_state.velocity.y * dt).abs() {
        // vertical motion was blocked by something, use the effective motion
        dynamic_state.velocity.y = target_motion.y / dt;
    }

    // clamp to max fall speed
    dynamic_state.velocity.y = dynamic_state.velocity.y.max(user.fall_speed);
}

// turn clipping on/off
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/idnoclip")]
pub(crate) struct NoClipCommand {
    clip: Option<bool>,
}

pub(crate) fn no_clip(mut input: ConsoleCommand<NoClipCommand>, mut clip: ResMut<UserClipping>) {
    if let Some(Ok(command)) = input.take() {
        let new_state = command.clip.unwrap_or(!clip.0);
        clip.0 = new_state;
        input.reply_ok(format!("clipping set to {}", clip.0));
    }
}

// set speed and friction
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/speed")]
pub(crate) struct SpeedCommand {
    run: f32,
    friction: f32,
}

pub(crate) fn speed_cmd(
    mut input: ConsoleCommand<SpeedCommand>,
    mut user: Query<&mut PrimaryUser>,
) {
    if let Some(Ok(command)) = input.take() {
        let mut user = user.single_mut();
        user.run_speed = command.run;
        user.friction = command.friction;
        input.reply_ok(format!(
            "run speed: {}, friction: {}",
            command.run, command.friction
        ));
    }
}

// set jump height, gravity, max fall speed
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/jump")]
pub(crate) struct JumpCommand {
    jump_height: f32,
    gravity: f32,
    fall_speed: f32,
}

pub(crate) fn jump_cmd(mut input: ConsoleCommand<JumpCommand>, mut user: Query<&mut PrimaryUser>) {
    if let Some(Ok(command)) = input.take() {
        let mut user = user.single_mut();
        user.jump_height = command.jump_height;
        user.gravity = -command.gravity;
        user.fall_speed = -command.fall_speed;
        input.reply_ok(format!(
            "jump height: {}, gravity: -{}, max fallspeed: -{}",
            command.jump_height, command.gravity, command.fall_speed
        ));
    }
}
