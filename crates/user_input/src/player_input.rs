use bevy::{math::Vec3Swizzles, prelude::*};

use common::{
    dynamics::PLAYER_GROUND_THRESHOLD,
    structs::{CameraOverride, CinematicControl, PrimaryCamera, PrimaryUser},
};

use avatar::AvatarDynamicState;
use dcl_component::proto_components::sdk::components::common::InputAction;
use input_manager::InputManager;

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn update_user_velocity(
    camera: Query<(&Transform, &PrimaryCamera)>,
    mut player: Query<(&Transform, &mut AvatarDynamicState, &PrimaryUser)>,
    input: InputManager,
) {
    let (Ok((player_transform, mut dynamic_state, user)), Ok((camera_transform, options))) =
        (player.get_single_mut(), camera.get_single())
    else {
        return;
    };

    // Handle key input
    if input.is_down(InputAction::IaJump)
        && dynamic_state.ground_height < PLAYER_GROUND_THRESHOLD
        && dynamic_state.velocity.y <= 0.0
    {
        dynamic_state.velocity.y = (user.jump_height * -user.gravity * 2.0).sqrt();
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

    // Apply movement update
    let (relative_transform, rotate) =
        if let Some(CameraOverride::Cinematic(cine)) = options.scene_override.as_ref() {
            match cine.avatar_control {
                CinematicControl::None => return,
                CinematicControl::Relative => (camera_transform, false),
                CinematicControl::Tank => (player_transform, true),
            }
        } else {
            (camera_transform, false)
        };

    dynamic_state.force = Vec2::ZERO;

    if axis_input != Vec2::ZERO {
        let max_speed = if !input.is_down(InputAction::IaWalk) {
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
        if rotate {
            dynamic_state.tank = true;
            dynamic_state.rotate = axis_input.x;
        } else {
            dynamic_state.tank = false;
            dynamic_state.rotate = 0.0;
            axis_output += right * axis_input.x;
        }
        dynamic_state.force = axis_output * max_speed;
    }
}
