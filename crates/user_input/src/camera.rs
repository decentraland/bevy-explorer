use std::f32::consts::PI;

use bevy::{
    input::mouse::{MouseMotion, MouseWheel},
    prelude::*,
    window::CursorGrabMode,
};

use common::structs::{CameraOverride, PrimaryCamera, PrimaryUser};
use input_manager::AcceptInput;
use scene_runner::{
    renderer_context::RendererSceneContext, update_world::mesh_collider::SceneColliderData,
    ContainingScene,
};

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn update_camera(
    time: Res<Time>,
    mut windows: Query<&mut Window>,
    mut mouse_events: EventReader<MouseMotion>,
    mut wheel_events: EventReader<MouseWheel>,
    mouse_button_input: Res<Input<MouseButton>>,
    key_input: Res<Input<KeyCode>>,
    mut move_toggled: Local<bool>,
    mut camera: Query<(&mut Transform, &mut PrimaryCamera)>,
    mut locked_cursor_position: Local<Option<Vec2>>,
    accept_input: Res<AcceptInput>,
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

    if accept_input.key {
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
    }

    // Handle mouse input
    let mut mouse_delta = Vec2::ZERO;
    if accept_input.mouse && mouse_button_input.pressed(options.mouse_key_enable_mouse) || *move_toggled {
        for mut window in &mut windows {
            if !window.focused {
                continue;
            }

            window.cursor.grab_mode = CursorGrabMode::Locked;
            window.cursor.visible = false;

            #[cfg(target_os = "windows")]
            {
                let cursor_position = locked_cursor_position
                    .get_or_insert_with(|| window.cursor_position().unwrap_or_default());
                window.set_cursor_position(Some(*cursor_position));
            }
        }

        for mouse_event in mouse_events.iter() {
            mouse_delta += mouse_event.delta;
        }
    }

    if mouse_button_input.just_released(options.mouse_key_enable_mouse)
        || (accept_input.key && key_input.just_pressed(options.keyboard_key_enable_mouse) && !*move_toggled)
    {
        for mut window in &mut windows {
            window.cursor.grab_mode = CursorGrabMode::None;
            window.cursor.visible = true;
            *locked_cursor_position = None;
        }
    }

    if accept_input.mouse {
        if let Some(event) = wheel_events.iter().last() {
            if event.y > 0.0 {
                options.distance = 0f32.max((options.distance - 0.05) * 0.9);
            } else if event.y < 0.0 {
                options.distance = 100f32.min((options.distance / 0.9) + 0.05);
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
    containing_scene: ContainingScene,
    mut scene_colliders: Query<(&RendererSceneContext, &mut SceneColliderData)>,
) {
    let (
        Ok(player_transform),
        Ok((mut camera_transform, options)),
    ) = (player.get_single_mut(), camera.get_single_mut()) else {
        return;
    };

    if let Some(CameraOverride::Cinematic(transform)) = options.scene_override {
        *camera_transform = transform;
    } else {
        let distance = match options.scene_override {
            Some(CameraOverride::Distance(d)) => d,
            _ => options.distance,
        };

        let player_head = player_transform.translation + Vec3::Y * 1.81;
        let target_direction =
            Vec3::Y * 0.2 * distance + camera_transform.rotation.mul_vec3(Vec3::Z * 5.0 * distance);
        let mut distance = target_direction.length();
        if target_direction.y + player_head.y < 0.0 {
            distance = distance * player_head.y / -target_direction.y;
        }
        let target_direction = target_direction.normalize_or_zero();

        if distance > 0.0 {
            // cast to check visibility
            if let Some((context, mut colliders)) = containing_scene
                .get_position(player_head)
                .and_then(|root| scene_colliders.get_mut(root).ok())
            {
                if let Some(hit) = colliders.cast_ray_nearest(
                    context.last_update_frame,
                    player_head,
                    target_direction.normalize(),
                    distance,
                    u32::MAX,
                ) {
                    distance = hit.toi;
                }
            }
            if let Some((context, mut colliders)) = containing_scene
                .get_position(player_head + target_direction * distance)
                .and_then(|root| scene_colliders.get_mut(root).ok())
            {
                if let Some(hit) = colliders.cast_ray_nearest(
                    context.last_update_frame,
                    player_head,
                    target_direction.normalize(),
                    distance,
                    u32::MAX,
                ) {
                    distance = hit.toi;
                }
            }
        }

        camera_transform.translation = player_head + target_direction * distance;
    }
}
