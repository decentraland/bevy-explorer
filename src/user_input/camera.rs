use std::f32::consts::PI;

use bevy::{
    input::mouse::{MouseMotion, MouseWheel},
    prelude::*,
    window::CursorGrabMode,
};

use crate::common::{PrimaryCamera, PrimaryUser, UiRoot};

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn update_camera(
    time: Res<Time>,
    mut windows: Query<&mut Window>,
    mut mouse_events: EventReader<MouseMotion>,
    mut wheel_events: EventReader<MouseWheel>,
    ui_root: Query<&Interaction, With<UiRoot>>,
    mouse_button_input: Res<Input<MouseButton>>,
    key_input: Res<Input<KeyCode>>,
    mut move_toggled: Local<bool>,
    mut camera: Query<(&mut Transform, &mut PrimaryCamera)>,
    mut locked_cursor_position: Local<Option<Vec2>>,
) {
    let dt = time.delta_seconds();

    let Ok((mut camera_transform, mut options)) = camera.get_single_mut() else {
        return;
    };

    if !options.initialized {
        let (yaw, pitch, roll) = camera_transform.rotation.to_euler(EulerRot::YXZ);
        options.yaw = yaw;
        options.pitch = pitch;
        options.roll = roll;
        options.initialized = true;
    }

    if key_input.just_pressed(options.keyboard_key_enable_mouse) {
        *move_toggled = !*move_toggled;
    }

    if key_input.pressed(options.key_roll_left) {
        options.roll += dt * 1.0;
    } else if key_input.pressed(options.key_roll_right) {
        options.roll -= dt * 1.0;
    } else if options.roll > 0.0 {
        options.roll = (options.roll - dt * 0.25).max(0.0);
    } else {
        options.roll = (options.roll + dt * 0.25).min(0.0);
    }

    // Handle mouse input
    let mut mouse_delta = Vec2::ZERO;
    if mouse_button_input.pressed(options.mouse_key_enable_mouse) || *move_toggled {
        for mut window in &mut windows {
            if !window.focused {
                continue;
            }

            window.cursor.grab_mode = CursorGrabMode::Locked;
            window.cursor.visible = false;

            let cursor_position = locked_cursor_position
                .get_or_insert_with(|| window.cursor_position().unwrap_or_default());
            window.set_cursor_position(Some(*cursor_position));
        }

        for mouse_event in mouse_events.iter() {
            mouse_delta += mouse_event.delta;
        }
    }
    if mouse_button_input.just_released(options.mouse_key_enable_mouse)
        || (key_input.just_pressed(options.keyboard_key_enable_mouse) && !*move_toggled)
    {
        for mut window in &mut windows {
            window.cursor.grab_mode = CursorGrabMode::None;
            window.cursor.visible = true;
            *locked_cursor_position = None;
        }
    }

    if ui_root.get_single().ok() != Some(&Interaction::None) {
        if let Some(event) = wheel_events.iter().last() {
            if event.y > 0.0 {
                options.distance = 0f32.max((options.distance - 0.05) * 0.9);
            } else if event.y < 0.0 {
                options.distance = 1f32.min((options.distance / 0.9) + 0.05);
            }
        }
    }

    // Apply look update
    options.pitch =
        (options.pitch - mouse_delta.y * options.sensitivity / 1000.0).clamp(-PI / 2., PI / 2.);
    options.yaw -= mouse_delta.x * options.sensitivity / 1000.0;
    camera_transform.rotation =
        Quat::from_euler(EulerRot::YXZ, options.yaw, options.pitch, options.roll);
}

pub fn update_camera_position(
    mut camera: Query<(&mut Transform, &PrimaryCamera)>,
    mut player: Query<&Transform, (With<PrimaryUser>, Without<PrimaryCamera>)>,
) {
    let (
        Ok(player_transform),
        Ok((mut camera_transform, options)),
    ) = (player.get_single_mut(), camera.get_single_mut()) else {
        return;
    };

    camera_transform.translation = player_transform.translation
        + Vec3::Y * (1.81 + 0.2 * options.distance)
        + camera_transform
            .rotation
            .mul_vec3(Vec3::Z * 5.0 * options.distance);
}
