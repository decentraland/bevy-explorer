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
use tween::SystemTween;

use crate::CursorLocked;

static TRANSITION_TIME: f32 = 0.5;

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn update_camera(
    time: Res<Time>,
    mut windows: Query<&mut Window>,
    mut mouse_events: EventReader<MouseMotion>,
    mut wheel_events: EventReader<MouseWheel>,
    mouse_button_input: Res<ButtonInput<MouseButton>>,
    key_input: Res<ButtonInput<KeyCode>>,
    mut move_toggled: Local<bool>,
    mut camera: Query<(&Transform, &mut PrimaryCamera)>,
    mut locked_cursor_position: Local<Option<Vec2>>,
    accept_input: Res<AcceptInput>,
    mut cursor_locked: ResMut<CursorLocked>,
    mut cinematic_initial: Local<Option<(f32, f32, f32, f32)>>,
) {
    let dt = time.delta_seconds();

    let Ok((camera_transform, mut options)) = camera.get_single_mut() else {
        return;
    };

    if !options.initialized {
        let (yaw, pitch, roll) = camera_transform.rotation.to_euler(EulerRot::YXZ);
        options.yaw = yaw;
        options.pitch = pitch;
        options.roll = roll;
        options.initialized = true;
    }

    let mut allow_cam_move = true;
    let mut allow_cam_distance = true;

    // record/reset cinematic start state
    if let Some(CameraOverride::Cinematic(cine)) = options.scene_override.clone() {
        if cinematic_initial.is_none() {
            *cinematic_initial = Some((options.yaw, options.pitch, options.roll, options.distance));
            (options.yaw, options.pitch, options.roll) = (0.0, 0.0, 0.0);
        }
        allow_cam_move = cine.camera_control;
        allow_cam_distance = false;
    } else if let Some(initial) = cinematic_initial.take() {
        (options.yaw, options.pitch, options.roll, options.distance) = initial;
    }

    // Handle mouse input
    let mut mouse_delta = Vec2::ZERO;
    if accept_input.mouse && mouse_button_input.pressed(options.mouse_key_enable_mouse)
        || *move_toggled
    {
        for mut window in &mut windows {
            if !window.focused {
                continue;
            }

            window.cursor.grab_mode = CursorGrabMode::Locked;
            window.cursor.visible = false;
            cursor_locked.0 = true;

            #[cfg(target_os = "windows")]
            {
                let cursor_position = locked_cursor_position
                    .get_or_insert_with(|| window.cursor_position().unwrap_or_default());
                window.set_cursor_position(Some(*cursor_position));
            }
        }

        for mouse_event in mouse_events.read() {
            mouse_delta += mouse_event.delta;
        }
    }

    if mouse_button_input.just_released(options.mouse_key_enable_mouse)
        || (accept_input.key
            && key_input.just_pressed(options.keyboard_key_enable_mouse)
            && !*move_toggled)
    {
        for mut window in &mut windows {
            window.cursor.grab_mode = CursorGrabMode::None;
            window.cursor.visible = true;
            cursor_locked.0 = false;
            *locked_cursor_position = None;
        }
    }

    if allow_cam_move {
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

        options.pitch = (options.pitch - mouse_delta.y * options.sensitivity / 1000.0)
            .clamp(-PI / 2.1, PI / 2.1);
        options.yaw -= mouse_delta.x * options.sensitivity / 1000.0;

        if accept_input.mouse && allow_cam_distance {
            if let Some(event) = wheel_events.read().last() {
                if event.y > 0.0 {
                    options.distance = 0f32.max((options.distance - 0.05) * 0.9);
                } else if event.y < 0.0 {
                    options.distance = 100f32.min((options.distance / 0.9) + 0.05);
                }
            }
        }
    }
}

pub fn update_camera_position(
    mut commands: Commands,
    mut camera: Query<(
        Entity,
        &mut Transform,
        &PrimaryCamera,
        Option<&mut SystemTween>,
    )>,
    mut player: Query<&Transform, (With<PrimaryUser>, Without<PrimaryCamera>)>,
    containing_scene: ContainingScene,
    mut scene_colliders: Query<(&RendererSceneContext, &mut SceneColliderData)>,
    mut prev_override: Local<Option<CameraOverride>>,
) {
    let (Ok(player_transform), Ok((camera_ent, mut camera_transform, options, maybe_tween))) =
        (player.get_single_mut(), camera.get_single_mut())
    else {
        return;
    };

    let mut target_transform = *camera_transform;

    if let Some(CameraOverride::Cinematic(cine)) = options.scene_override.as_ref() {
        target_transform = cine.origin.compute_transform();
        target_transform.rotation *=
            Quat::from_euler(EulerRot::YXZ, options.yaw, options.pitch, options.roll);
    } else {
        let distance = match options.scene_override {
            Some(CameraOverride::Distance(d)) => d,
            _ => options.distance,
        };

        target_transform.rotation =
            Quat::from_euler(EulerRot::YXZ, options.yaw, options.pitch, options.roll);

        let xz_plane = (target_transform.rotation.mul_vec3(-Vec3::Z) * Vec3::new(1.0, 0.0, 1.0))
            .normalize_or_zero()
            * distance.clamp(0.0, 1.0);
        let player_head = player_transform.translation
            + Vec3::Y * 1.81
            + target_transform
                .rotation
                .mul_vec3(Vec3::new(1.0, -0.4, 0.0))
                * distance.clamp(0.0, 0.5)
            + xz_plane;

        let target_direction = target_transform.rotation.mul_vec3(Vec3::Z * 5.0 * distance);
        let mut distance = target_direction.length();
        if target_direction.y + player_head.y < 0.1 {
            distance = distance * (player_head.y - 0.1) / -target_direction.y;
        }
        let target_direction = target_direction.normalize_or_zero();

        if distance > 0.0 {
            // cast to check visibility
            let scenes_head = containing_scene.get_position(player_head);
            let scenes_cam =
                containing_scene.get_position(player_head + target_direction * distance);

            for scene in (scenes_head).union(&scenes_cam) {
                let Ok((context, mut colliders)) = scene_colliders.get_mut(*scene) else {
                    continue;
                };

                if let Some(hit) = colliders.cast_ray_nearest(
                    context.last_update_frame,
                    player_head - xz_plane,
                    target_direction.normalize(),
                    distance,
                    u32::MAX,
                    false,
                ) {
                    distance = distance.min(hit.toi - 0.1).max(0.0);
                }
            }
        }

        target_transform.translation = player_head + target_direction * distance;
    }

    if prev_override.as_ref().map(std::mem::discriminant)
        != options.scene_override.as_ref().map(std::mem::discriminant)
    {
        prev_override.clone_from(&options.scene_override);
        commands.entity(camera_ent).try_insert(SystemTween {
            target: target_transform,
            time: TRANSITION_TIME,
        });
    } else if let Some(mut tween) = maybe_tween {
        // bypass change detection so the tween state doesn't reset
        tween.bypass_change_detection().target = target_transform;
    } else {
        *camera_transform = target_transform;
    }
}
