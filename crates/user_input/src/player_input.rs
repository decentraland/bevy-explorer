use bevy::{math::Vec3Swizzles, prelude::*};

use common::{
    dynamics::PLAYER_GROUND_THRESHOLD,
    structs::{AvatarControl, AvatarDynamicState, PrimaryCamera, PrimaryUser},
};

use dcl_component::proto_components::sdk::components::common::InputAction;
use input_manager::InputManager;
use scene_runner::update_world::avatar_modifier_area::PlayerModifiers;

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
        (player.get_single_mut(), camera.get_single())
    else {
        return;
    };

    let user = maybe_modifiers
        .map(|m| m.combine(user))
        .unwrap_or_else(|| user.clone());

    // Handle key input
    if input.is_down(InputAction::IaJump)
        && dynamic_state.ground_height < PLAYER_GROUND_THRESHOLD
        && dynamic_state.velocity.y <= 0.0
    {
        dynamic_state.velocity.y = (user.jump_height * -user.gravity * 2.0).sqrt();
        dynamic_state.jump_time = time.elapsed_seconds();
    }

    let mut axis_input = Vec2::ZERO;
    if input.is_down(InputAction::IaForward) {
        axis_input.y += 1.0;
    }
    if input.is_down(InputAction::IaBackward) {
        axis_input.y -= 1.0;
    }
    if input.is_down(InputAction::IaRight) {
        axis_input.x += 1.0;
    }
    if input.is_down(InputAction::IaLeft) {
        axis_input.x -= 1.0;
    }

    dynamic_state.force = Vec2::ZERO;
    dynamic_state.rotate = 0.0;

    // Apply movement update
    let (relative_transform, rotate) = match user.control_type {
        AvatarControl::None => return,
        AvatarControl::Relative => (camera_transform, false),
        AvatarControl::Tank => (player_transform, true),
    };

    if rotate {
        *tankiness = (*tankiness + time.delta_seconds() / TRANSITION_TIME).min(1.0);
        dynamic_state.tank = true;
    } else {
        *tankiness = (*tankiness - time.delta_seconds() / TRANSITION_TIME).max(0.0);
        dynamic_state.tank = false;
    }

    if axis_input != Vec2::ZERO {
        let max_speed = if !input.is_down(InputAction::IaWalk) || user.block_weighted_movement {
            user.run_speed
        } else {
            user.walk_speed
        };
        axis_input = axis_input.normalize();

        let ground = Vec3::X + Vec3::Z;
        let forward = (Vec3::from(relative_transform.forward()) * ground)
            .xz()
            .normalize_or_zero();
        let right = (Vec3::from(relative_transform.right()) * ground)
            .xz()
            .normalize_or_zero();

        let mut axis_output = forward * axis_input.y;
        dynamic_state.rotate = -axis_input.x * *tankiness * user.turn_speed;
        axis_output += right * axis_input.x * (1.0 - *tankiness);

        dynamic_state.force = axis_output * max_speed;
    }
}
