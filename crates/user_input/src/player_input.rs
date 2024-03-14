use bevy::{math::Vec3Swizzles, prelude::*};

use common::{
    dynamics::{GRAVITY, MAX_JUMP_HEIGHT, PLAYER_GROUND_THRESHOLD},
    structs::{PrimaryCamera, PrimaryUser},
};

use avatar::AvatarDynamicState;
use dcl_component::proto_components::sdk::components::common::InputAction;
use input_manager::InputManager;

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn update_user_velocity(
    camera: Query<&Transform, With<PrimaryCamera>>,
    mut player: Query<(&mut AvatarDynamicState, &PrimaryUser)>,
    input: InputManager,
    time: Res<Time>,
) {
    let (Ok((mut dynamic_state, user)), Ok(camera_transform)) =
        (player.get_single_mut(), camera.get_single())
    else {
        return;
    };

    // Handle key input
    if input.is_down(InputAction::IaJump)
        && dynamic_state.ground_height < PLAYER_GROUND_THRESHOLD
        && dynamic_state.velocity.y <= 0.0
    {
        dynamic_state.velocity.y = (MAX_JUMP_HEIGHT * GRAVITY * 2.0).sqrt();
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
    if axis_input != Vec2::ZERO {
        let max_speed = if !input.is_down(InputAction::IaWalk) {
            user.run_speed
        } else {
            user.walk_speed
        };
        axis_input = axis_input.normalize() * max_speed * time.delta_seconds();

        let ground = Vec3::X + Vec3::Z;
        let forward = (Vec3::from(camera_transform.forward()) * ground)
            .xz()
            .normalize_or_zero();
        let right = (Vec3::from(camera_transform.right()) * ground).xz().normalize_or_zero();

        axis_input = right * axis_input.x + forward * axis_input.y;

        dynamic_state.velocity.x += axis_input.x;
        dynamic_state.velocity.z += axis_input.y;
    }
}
