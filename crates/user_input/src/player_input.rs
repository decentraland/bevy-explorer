use bevy::{math::Vec3Swizzles, prelude::*};

use common::{
    dynamics::PLAYER_GROUND_THRESHOLD,
    inputs::{CommonInputAction, MOVE_SET},
    structs::{AvatarControl, AvatarDynamicState, PlayerModifiers, PrimaryCamera, PrimaryUser},
};

use input_manager::{InputManager, InputPriority};

use crate::TRANSITION_TIME;

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn update_user_velocity(
    camera: Query<&Transform, With<PrimaryCamera>>,
    mut player: Query<(
        &Transform,
        &mut AvatarDynamicState,
        &PrimaryUser,
        Option<&PlayerModifiers>,
    )>,
    input: InputManager,
    mut tankiness: Local<f32>,
    time: Res<Time>,
) {
    let (Ok((player_transform, mut dynamic_state, user, maybe_modifiers)), Ok(camera_transform)) =
        (player.single_mut(), camera.single())
    else {
        return;
    };

    let user = maybe_modifiers
        .map(|m| m.combine(user))
        .unwrap_or_else(|| user.clone());

    // Handle key input
    let jump_key = input.is_down(CommonInputAction::IaJump, InputPriority::Scene)
        || input.just_down(CommonInputAction::IaJump, InputPriority::Scene);
    if jump_key {
        let jump_velocity = (user.jump_height * -user.gravity * 2.0).sqrt();
        if dynamic_state.ground_height < PLAYER_GROUND_THRESHOLD // grounded
            && dynamic_state.velocity.y <= jump_velocity * 0.1 // not already jumping
            && !user.block_jump
        // scene allowed to jump
        {
            dynamic_state.velocity.y = (user.jump_height * -user.gravity * 2.0).sqrt();
            dynamic_state.jump_time = time.elapsed_secs();
        } else {
            debug!(
                "jump failed:\nground height {} < {}? {}\ny velocity: {} < {}? {}, blocked: {}",
                dynamic_state.ground_height,
                PLAYER_GROUND_THRESHOLD,
                dynamic_state.ground_height < PLAYER_GROUND_THRESHOLD,
                dynamic_state.velocity.y,
                (user.jump_height * -user.gravity * 2.0).sqrt() * 0.1,
                dynamic_state.velocity.y <= (user.jump_height * -user.gravity * 2.0).sqrt() * 0.1,
                user.block_jump
            );
        }
    }

    let axis_input = input.get_analog(MOVE_SET, InputPriority::Scene);

    dynamic_state.force = Vec2::ZERO;
    dynamic_state.rotate = 0.0;

    // Apply movement update
    let (relative_transform, rotate) = match user.control_type {
        AvatarControl::None => return,
        AvatarControl::Relative => (camera_transform, false),
        AvatarControl::Tank => (player_transform, true),
    };

    if rotate {
        *tankiness = (*tankiness + time.delta_secs() / TRANSITION_TIME).min(1.0);
        dynamic_state.tank = true;
    } else {
        *tankiness = (*tankiness - time.delta_secs() / TRANSITION_TIME).max(0.0);
        dynamic_state.tank = false;
    }

    if axis_input != Vec2::ZERO && !user.block_run {
        let movement_axis = match (user.block_walk, user.block_run, user.block_all) {
            (_, _, true) | (true, true, false) => Vec2::ZERO,
            (true, false, false) => axis_input.normalize_or_zero() * user.run_speed,
            (false, true, false) => axis_input / axis_input.length().max(1.0) * user.walk_speed,
            (false, false, false) => {
                axis_input / axis_input.length().max(1.0)
                    * if input.is_down(CommonInputAction::IaWalk, InputPriority::Scene) {
                        user.walk_speed
                    } else {
                        user.run_speed
                    }
            }
        };

        let ground = Vec3::X + Vec3::Z;
        let forward = (Vec3::from(relative_transform.forward()) * ground)
            .xz()
            .normalize_or_zero();
        let right = (Vec3::from(relative_transform.right()) * ground)
            .xz()
            .normalize_or_zero();

        if !user.block_all {
            dynamic_state.rotate = -axis_input.x * *tankiness * user.turn_speed;
        }
        let axis_output = forward * movement_axis.y + right * movement_axis.x * (1.0 - *tankiness);

        dynamic_state.force = axis_output;
    }
}
