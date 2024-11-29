use std::{
    f32::consts::{FRAC_PI_4, PI},
    marker::PhantomData,
};

use avatar::AvatarDynamicState;
use bevy::{
    ecs::system::SystemParam,
    input::mouse::{MouseMotion, MouseWheel},
    prelude::*,
    utils::HashMap,
    window::CursorGrabMode,
};

use common::{
    structs::{ActiveDialog, CameraOverride, PrimaryCamera, PrimaryUser},
    util::ModifyComponentExt,
};
use input_manager::AcceptInput;
use scene_runner::{
    renderer_context::RendererSceneContext, update_world::mesh_collider::SceneColliderData,
    ContainingScene,
};
use tween::SystemTween;
use ui_core::scrollable::UsedScrollWheel;

use crate::TRANSITION_TIME;

#[allow(clippy::too_many_arguments, clippy::type_complexity)]

pub struct CinematicInitialData {
    base_yaw: f32,
    base_pitch: f32,
    base_roll: f32,
    base_distance: f32,
    cinematic_transform: GlobalTransform,
}

#[derive(SystemParam)]
pub struct MouseInteractionState<'w, 's> {
    mouse_button_input: Res<'w, ButtonInput<MouseButton>>,
    states: Local<'s, HashMap<MouseButton, (ClickState, f32)>>,
    time: Res<'w, Time>,
    #[system_param(ignore)]
    _p: PhantomData<&'s ()>,
}

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClickState {
    #[default]
    None,
    Clicked,
    Held,
    Released,
}

impl<'w, 's> MouseInteractionState<'w, 's> {
    pub fn update(&mut self, button: MouseButton) -> ClickState {
        let state = self.states.entry(button).or_default();

        match state.0 {
            ClickState::None | ClickState::Released => {
                if self.mouse_button_input.just_pressed(button) {
                    *state = (ClickState::Held, self.time.elapsed_seconds());
                } else {
                    state.0 = ClickState::None;
                }
            }
            ClickState::Held => {
                if self.mouse_button_input.just_released(button) {
                    if self.time.elapsed_seconds() - state.1 > 0.25 {
                        state.0 = ClickState::Released;
                    } else {
                        state.0 = ClickState::Clicked;
                    }
                }
            }
            ClickState::Clicked => state.0 = ClickState::Released,
        }

        state.0
    }
}

#[allow(clippy::too_many_arguments)]
pub fn update_camera(
    time: Res<Time>,
    mut windows: Query<&mut Window>,
    mut mouse_events: EventReader<MouseMotion>,
    mut wheel_events: EventReader<MouseWheel>,
    key_input: Res<ButtonInput<KeyCode>>,
    mut move_toggled: Local<bool>,
    mut camera: Query<(&Transform, &mut PrimaryCamera)>,
    mut locked_cursor_position: Local<Option<Vec2>>,
    accept_input: Res<AcceptInput>,
    used_wheel: Res<UsedScrollWheel>,
    mut cursor_locked: ResMut<system_ui::sysinfo::CursorLocked>,
    active_dialog: Res<ActiveDialog>,
    mut cinematic_data: Local<Option<CinematicInitialData>>,
    mut mb_state: MouseInteractionState,
    gt_helper: TransformHelper,
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

    let mut yaw_range = None;
    let mut pitch_range = None;
    let mut roll_range = None;
    let mut zoom_range = None;

    // record/reset cinematic start state
    if let Some(CameraOverride::Cinematic(cine)) = options.scene_override.clone() {
        let Ok(origin) = gt_helper.compute_global_transform(cine.origin) else {
            warn!("failed to get gt");
            return;
        };

        let (scale, _, _) = origin.to_scale_rotation_translation();
        let cinematic_distance = scale.z;

        match cinematic_data.as_mut() {
            None => {
                *cinematic_data = Some(CinematicInitialData {
                    base_yaw: options.yaw,
                    base_pitch: options.pitch,
                    base_roll: options.roll,
                    base_distance: options.distance,
                    cinematic_transform: origin,
                });

                options.distance = cinematic_distance;
            }
            Some(ref mut existing) => {
                if existing.cinematic_transform != origin {
                    // reset for updated transform
                    let (scale, _, _) =
                        existing.cinematic_transform.to_scale_rotation_translation();
                    let prev_distance = scale.z;
                    options.distance = cinematic_distance + options.distance - prev_distance;
                    existing.cinematic_transform = origin;
                }
            }
        }

        allow_cam_move = cine.allow_manual_rotation;
        yaw_range = cine.yaw_range.map(|r| (-r..r));
        pitch_range = cine.pitch_range.map(|r| (-r..r));
        roll_range = cine.roll_range.map(|r| (-r..r));
        zoom_range = Some(
            cine.zoom_min.unwrap_or(scale.z).clamp(0.3, 100.0)
                ..cine.zoom_max.unwrap_or(scale.z).clamp(0.3, 100.0),
        );
    } else if let Some(initial) = cinematic_data.take() {
        (options.yaw, options.pitch, options.roll, options.distance) = (
            initial.base_yaw,
            initial.base_pitch,
            initial.base_roll,
            initial.base_distance,
        );
    }

    // Handle mouse input
    let mut state = mb_state.update(options.mouse_key_enable_mouse);
    if key_input.just_pressed(KeyCode::Escape) && *move_toggled {
        // override
        state = ClickState::Released;
        *move_toggled = false;
    }

    let mut mouse_delta = Vec2::ZERO;

    let in_dialog = active_dialog.in_use();
    let lock = !in_dialog && (accept_input.mouse && state == ClickState::Held || *move_toggled);

    if lock {
        for mut window in &mut windows {
            if !window.focused {
                continue;
            }

            window.cursor.grab_mode = CursorGrabMode::Locked;
            window.cursor.visible = false;
            if !in_dialog {
                cursor_locked.0 = true;
            }

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

    if state == ClickState::Released || in_dialog {
        for mut window in &mut windows {
            window.cursor.grab_mode = CursorGrabMode::None;
            window.cursor.visible = true;
            if !in_dialog {
                cursor_locked.0 = false;
            }
            *locked_cursor_position = None;
        }
    }

    if allow_cam_move {
        if state == ClickState::Clicked {
            *move_toggled = !*move_toggled;
        }

        if accept_input.key {
            if key_input.pressed(options.key_roll_left) {
                options.roll += dt * 1.0;
            } else if key_input.pressed(options.key_roll_right) {
                options.roll -= dt * 1.0;
            } else {
                // decay roll if not in cinematic mode
                if options.roll > 0.0 {
                    options.roll = (options.roll - dt * 0.25).max(0.0);
                } else {
                    options.roll = (options.roll + dt * 0.25).min(0.0);
                }
            }
        }

        options.pitch = (options.pitch - mouse_delta.y * options.sensitivity / 1000.0)
            .clamp(-PI / 2.1, PI / 2.1);
        options.yaw -= mouse_delta.x * options.sensitivity / 1000.0;
        if accept_input.mouse && !used_wheel.0 {
            if let Some(event) = wheel_events.read().last() {
                if (event.y > 0.0) == zoom_range.is_none() {
                    options.distance = 0f32.max((options.distance - 0.05) * 0.9);
                } else {
                    options.distance = 7000f32.min((options.distance / 0.9) + 0.05);
                }
            }
        }
    }

    if let Some(roll_range) = roll_range {
        options.roll = options.roll.clamp(roll_range.start, roll_range.end);
    }
    if let Some(pitch_range) = pitch_range {
        options.pitch = options.pitch.clamp(pitch_range.start, pitch_range.end);
    }
    if let Some(yaw_range) = yaw_range {
        options.yaw = options.yaw.clamp(yaw_range.start, yaw_range.end);
    }
    if let Some(zoom_range) = zoom_range {
        options.distance = options.distance.clamp(zoom_range.start, zoom_range.end);
    }
}

#[allow(clippy::type_complexity)]
pub fn update_camera_position(
    mut commands: Commands,
    mut camera: Query<(
        Entity,
        &Transform,
        &PrimaryCamera,
        &mut Projection,
        Option<&mut SystemTween>,
    )>,
    mut player: Query<
        (&Transform, &AvatarDynamicState),
        (With<PrimaryUser>, Without<PrimaryCamera>),
    >,
    containing_scene: ContainingScene,
    mut scene_colliders: Query<(&RendererSceneContext, &mut SceneColliderData)>,
    mut prev_override: Local<Option<CameraOverride>>,
    gt_helper: TransformHelper,
) {
    let (
        Ok((player_transform, dynamic_state)),
        Ok((camera_ent, camera_transform, options, mut projection, maybe_tween)),
    ) = (player.get_single_mut(), camera.get_single_mut())
    else {
        return;
    };

    let mut target_transform = *camera_transform;

    if let Some(CameraOverride::Cinematic(cine)) = options.scene_override.as_ref() {
        let Ok(origin) = gt_helper.compute_global_transform(cine.origin) else {
            warn!("failed to get gt");
            return;
        };

        let (_, rotation, translation) = origin.to_scale_rotation_translation();

        target_transform.translation = translation;
        target_transform.rotation =
            rotation * Quat::from_euler(EulerRot::YXZ, options.yaw, options.pitch, options.roll);
        let target_fov = FRAC_PI_4 * 1.25 / options.distance;
        let Projection::Perspective(PerspectiveProjection { ref mut fov, .. }) = &mut *projection
        else {
            panic!();
        };
        if *fov != target_fov {
            *fov = target_fov;
        }
    } else {
        let target_fov = (dynamic_state.velocity.length() / 4.0).clamp(1.25, 1.25) * FRAC_PI_4;
        if let Projection::Perspective(PerspectiveProjection { ref mut fov, .. }) = &mut *projection
        {
            if *fov != target_fov {
                *fov = target_fov;
            }
        };

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
        commands
            .entity(camera_ent)
            .modify_component(move |t: &mut Transform| *t = target_transform);
        // *camera_transform = target_transform;
    }
}
