use std::f32::consts::PI;

use bevy::{core::FrameCount, math::Vec3Swizzles, prelude::*};
use rapier3d::control::{CharacterAutostep, CharacterLength, KinematicCharacterController};

use crate::{
    avatar::AvatarDynamicState,
    scene_runner::{
        renderer_context::RendererSceneContext, update_world::mesh_collider::SceneColliderData,
        ContainingScene, PrimaryUser,
    },
};
pub const GRAVITY: f32 = 20.0;
pub const MAX_FALL_SPEED: f32 = 15.0;
pub const MAX_CLIMBABLE_INCLINE: f32 = 1.5 * PI / 4.0; // radians from up - equal to 60 degree incline
pub const MAX_STEP_HEIGHT: f32 = 0.5;
pub const MAX_JUMP_HEIGHT: f32 = 1.25;
pub const PLAYER_GROUND_THRESHOLD: f32 = 0.05;

pub const PLAYER_COLLIDER_RADIUS: f32 = 0.35;
pub const PLAYER_COLLIDER_HEIGHT: f32 = 2.0;
pub const PLAYER_COLLIDER_OVERLAP: f32 = 0.01;

pub fn update_user_position(
    mut player: Query<(
        Entity,
        &PrimaryUser,
        &mut Transform,
        &mut AvatarDynamicState,
    )>,
    mut scene_datas: Query<(
        &mut RendererSceneContext,
        &mut SceneColliderData,
        &GlobalTransform,
    )>,
    containing_scene: ContainingScene,
    time: Res<Time>,
    _frame: Res<FrameCount>,
) {
    let Ok((user_ent, user, mut transform, mut dynamic_state)) = player.get_single_mut() else {
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

    // get containing scene
    let scene = containing_scene.get(user_ent);
    match scene.and_then(|scene| scene_datas.get_mut(scene).ok()) {
        None => {
            // no scene, just update translation directly
            transform.translation += dynamic_state.velocity * dt;

            if transform.translation.y > 0.0 {
                dynamic_state.velocity.y -= half_g_force;
            } else {
                dynamic_state.velocity.y = 0f32.max(dynamic_state.velocity.y - half_g_force);
            }

            dynamic_state.ground_height = transform.translation.y;
        }
        Some((context, mut collider_data, _scene_transform)) => {
            // setup physics controller
            let mut controller = KinematicCharacterController {
                offset: CharacterLength::Absolute(PLAYER_COLLIDER_OVERLAP),
                slide: true,
                autostep: Some(CharacterAutostep {
                    max_height: CharacterLength::Absolute(MAX_STEP_HEIGHT),
                    min_width: CharacterLength::Relative(0.75),
                    include_dynamic_bodies: true,
                }),
                max_slope_climb_angle: MAX_CLIMBABLE_INCLINE,
                min_slope_slide_angle: MAX_CLIMBABLE_INCLINE,
                snap_to_ground: Some(CharacterLength::Absolute(0.1)),
                ..Default::default()
            };

            // unset autostep when jumping
            if dynamic_state.velocity.y > 0.0 {
                controller.autostep = None;
            }

            // get allowed movement
            let eff_movement = collider_data.move_character(
                context.last_update_frame,
                transform.translation,
                dynamic_state.velocity * dt,
                &controller,
            );

            transform.translation += Vec3::from(eff_movement.translation);
            transform.translation.y = transform.translation.y.max(0.0);

            // calculate ground height
            (dynamic_state.ground_height, dynamic_state.ground_collider) = match collider_data
                .get_groundheight(context.last_update_frame, transform.translation)
            {
                Some((height, collider)) => (height, Some((scene.unwrap(), collider))),
                None => (transform.translation.y, None),
            };

            // update vertical velocity
            if dynamic_state.ground_height <= 0.0 || transform.translation.y == 0.0 {
                // on the floor, set vertical velocity to zero
                dynamic_state.velocity.y = dynamic_state.velocity.y.max(0.0);
            } else if eff_movement.translation.y.abs() < (0.5 * dynamic_state.velocity.y * dt).abs()
            {
                // vertical motion was blocked by something, use the effective motion
                dynamic_state.velocity.y = eff_movement.translation.y / dt - half_g_force;
            } else {
                dynamic_state.velocity.y -= half_g_force;
            }
        }
    };

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
