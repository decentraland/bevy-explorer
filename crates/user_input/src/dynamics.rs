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

use bevy::{core::FrameCount, math::Vec3Swizzles, prelude::*};
use bevy_console::ConsoleCommand;
use rapier3d_f64::control::{CharacterAutostep, CharacterLength, KinematicCharacterController};

use common::{
    dynamics::{
        GRAVITY, MAX_CLIMBABLE_INCLINE, MAX_FALL_SPEED, MAX_STEP_HEIGHT, PLAYER_COLLIDER_OVERLAP,
        PLAYER_GROUND_THRESHOLD,
    },
    structs::PrimaryUser,
};

use avatar::AvatarDynamicState;
use scene_runner::{
    renderer_context::RendererSceneContext,
    update_world::mesh_collider::{ColliderId, GroundCollider, SceneColliderData},
    ContainingScene, OutOfWorld,
};

#[derive(Resource)]
pub struct UserClipping(pub bool);

pub fn update_user_position(
    mut player: Query<
        (
            Entity,
            &PrimaryUser,
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
) {
    let Ok((user_ent, user, mut transform, mut dynamic_state, mut ground_collider)) =
        player.get_single_mut()
    else {
        return;
    };

    let dt = time.delta_seconds();
    // we apply half gravity before motion and half after to avoid (significant) max height difference due to frame rate
    let half_g_force = dt * GRAVITY * 0.5;
    if dynamic_state.velocity.y != 0.0 {
        dynamic_state.velocity.y -= half_g_force;
    }

    // rotate towards velocity vec
    let target_xz = dynamic_state.velocity.xz() * dt;
    if target_xz.length() > 0.0 {
        let target_rotation = Transform::default()
            .looking_at(dynamic_state.velocity * (Vec3::X + Vec3::Z), Vec3::Y)
            .rotation;

        transform.rotation = transform.rotation.lerp(target_rotation, dt * 10.0);
    }

    let velocity_motion = dynamic_state.velocity * dt;
    let mut platform_motion = Vec3::default();
    let mut target_motion = velocity_motion;
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

                // force update new collider
                collider_data.update_collider_transform(&collider, &new_global_transform, None);

                // adjust base motion wrt ground collider
                if clip.0 {
                    target_motion = collider_data.move_character(
                        ctx.last_update_frame,
                        transform.translation,
                        target_motion,
                        &controller,
                        Some(&collider),
                        true,
                    );    
                }

                // add platform motion
                target_motion += platform_motion;

                platform_handle = Some((scene_ent, collider));
            }
        }
    }

    // check containing scenes
    for scene in containing_scenes.get(user_ent) {
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
        velocity_motion,
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
                }
            }
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
        dynamic_state.velocity.y = target_motion.y / dt - half_g_force;
    } else {
        dynamic_state.velocity.y -= half_g_force;
    }

    // cap fall speed
    dynamic_state.velocity.y = dynamic_state.velocity.y.max(-MAX_FALL_SPEED);

    // friction
    let mult = user.friction.recip().powf(dt);
    dynamic_state.velocity.x *= mult;
    dynamic_state.velocity.z *= mult;

    if dynamic_state.velocity.xz().length_squared() < 1e-3 {
        dynamic_state.velocity.x = 0.0;
        dynamic_state.velocity.z = 0.0;
    }
}

// turn clipping on/off
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/idnoclip")]
pub(crate) struct NoClipCommand {
    clip: Option<bool>,
}

pub(crate) fn no_clip(
    mut input: ConsoleCommand<NoClipCommand>,
    mut clip: ResMut<UserClipping>,
) {
    if let Some(Ok(command)) = input.take() {
        let new_state = command.clip.unwrap_or(!clip.0);
        clip.0 = new_state;
        input.reply_ok(format!("clipping set to {}", clip.0));
    }
}
