use std::f32::consts::{FRAC_PI_4, PI};

use bevy::{
    math::FloatOrd,
    prelude::*,
    window::{CursorGrabMode, PrimaryWindow},
};

use common::{
    inputs::{Action, SystemAction, CAMERA_SET, CAMERA_ZOOM, POINTER_SET},
    structs::{AvatarDynamicState, CameraOverride, CursorLocks, PrimaryCamera, PrimaryUser},
    util::ModifyComponentExt,
};
use dcl_component::proto_components::sdk::components::common::camera_transition::TransitionMode;
use input_manager::{InputManager, InputPriority};
use scene_runner::{
    renderer_context::RendererSceneContext, update_world::mesh_collider::SceneColliderData,
    ContainingScene, OutOfWorld,
};
use tween::SystemTween;

use crate::TRANSITION_TIME;

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub struct CinematicInitialData {
    base_yaw: f32,
    base_pitch: f32,
    base_roll: f32,
    base_distance: f32,
    cinematic_transform: GlobalTransform,
}

#[allow(clippy::too_many_arguments)]
pub fn update_camera(
    time: Res<Time>,
    mut camera: Query<(&Transform, &mut PrimaryCamera)>,
    locks: Res<CursorLocks>,
    mut cinematic_data: Local<Option<CinematicInitialData>>,
    input_manager: InputManager,
    gt_helper: TransformHelper,
) {
    let dt = time.delta_secs();

    let Ok((camera_transform, mut options)) = camera.single_mut() else {
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
                options.yaw = 0.0;
                options.pitch = 0.0;
                options.roll = 0.0;
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
        yaw_range = cine.yaw_range.map(|r| -r..r);
        pitch_range = cine.pitch_range.map(|r| -r..r);
        roll_range = cine.roll_range.map(|r| -r..r);
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

    let mut mouse_delta = input_manager.get_analog(CAMERA_SET, InputPriority::Scene) * 10.0;
    if locks.0.contains("camera") {
        mouse_delta += input_manager.get_analog(POINTER_SET, InputPriority::Scroll);
    }

    if allow_cam_move {
        if input_manager.is_down(Action::System(SystemAction::RollLeft), InputPriority::None) {
            options.roll += dt * 1.0;
        } else if input_manager
            .is_down(Action::System(SystemAction::RollRight), InputPriority::None)
        {
            options.roll -= dt * 1.0;
        } else {
            // decay roll if not in cinematic mode
            if options.roll > 0.0 {
                options.roll = (options.roll - dt * 0.25).max(0.0);
            } else {
                options.roll = (options.roll + dt * 0.25).min(0.0);
            }
        }

        options.pitch = (options.pitch - mouse_delta.y * options.sensitivity / 1000.0)
            .clamp(-PI / 2.1, PI / 2.1);
        options.yaw -= mouse_delta.x * options.sensitivity / 1000.0;
        let zoom = input_manager
            .get_analog(CAMERA_ZOOM, InputPriority::Scene)
            .y;
        if zoom != 0.0 {
            let zoom = zoom.clamp(-1000.0, 1000.0);
            options.distance =
                (((options.distance + 0.5) * 1.0005f32.powf(-zoom)) - 0.5).clamp(0.0, 100.0);
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

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn update_camera_position(
    mut commands: Commands,
    mut camera: Query<(
        Entity,
        &Transform,
        &PrimaryCamera,
        &mut Projection,
        Option<&mut SystemTween>,
    )>,
    player: Query<
        (&Transform, &AvatarDynamicState, Has<OutOfWorld>),
        (With<PrimaryUser>, Without<PrimaryCamera>),
    >,
    containing_scene: ContainingScene,
    mut scene_colliders: Query<(&RendererSceneContext, &mut SceneColliderData)>,
    mut prev_override: Local<Option<CameraOverride>>,
    mut prev_oow: Local<bool>,
    gt_helper: TransformHelper,
) {
    let (
        Ok((player_transform, dynamic_state, is_oow)),
        Ok((camera_ent, camera_transform, options, mut projection, maybe_tween)),
    ) = (player.single(), camera.single_mut())
    else {
        return;
    };

    let mut target_transform = *camera_transform;
    let mut target_transition = TransitionMode::Time(TRANSITION_TIME);

    if is_oow {
        target_transform = Transform::from_translation(
            player_transform.translation + Vec3::new(15.0, 75.0, 100.0),
        )
        .looking_at(player_transform.translation, Vec3::Y);
    } else if let Some(CameraOverride::Cinematic(cine)) = options.scene_override.as_ref() {
        let Ok(origin) = gt_helper.compute_global_transform(cine.origin) else {
            warn!("failed to get gt");
            return;
        };

        let (_, rotation, translation) = origin.to_scale_rotation_translation();

        target_transform.translation = translation;
        target_transform.rotation = if let Some(look_at_transform) = cine
            .look_at_entity
            .and_then(|e| gt_helper.compute_global_transform(e).ok())
        {
            Transform::IDENTITY
                .looking_at(
                    look_at_transform.translation() - camera_transform.translation,
                    Vec3::Y,
                )
                .rotation
        } else {
            let yaw = cine
                .yaw_range
                .map(|r| options.yaw.clamp(-r, r))
                .unwrap_or(options.yaw);
            let pitch = cine
                .yaw_range
                .map(|r| options.pitch.clamp(-r, r))
                .unwrap_or(options.pitch);
            let roll = cine
                .yaw_range
                .map(|r| options.roll.clamp(-r, r))
                .unwrap_or(options.roll);
            rotation * Quat::from_euler(EulerRot::YXZ, yaw, pitch, roll)
        };
        let target_fov = FRAC_PI_4 * 1.25 / options.distance;
        let Projection::Perspective(PerspectiveProjection { ref mut fov, .. }) = &mut *projection
        else {
            panic!();
        };
        if *fov != target_fov {
            *fov = target_fov;
        }
        if let Some(transition) = cine
            .transition
            .as_ref()
            .and_then(|ct| ct.transition_mode.as_ref())
        {
            target_transition = transition.clone();
        }
    } else {
        let target_fov = (dynamic_state.velocity.length() / 4.0).clamp(1.25, 1.25) * FRAC_PI_4;
        if let Projection::Perspective(PerspectiveProjection { ref mut fov, .. }) = &mut *projection
        {
            if *fov != target_fov {
                *fov = target_fov;
            }
        };

        let mut distance = match options.scene_override {
            Some(CameraOverride::Distance(d)) => d,
            _ => options.distance,
        } * 5.0;

        target_transform.rotation =
            Quat::from_euler(EulerRot::YXZ, options.yaw, options.pitch, options.roll);

        let player_head = player_transform.translation + Vec3::Y * 1.81;
        let head_offset = (target_transform.rotation.mul_vec3(Vec3::X) * Vec3::new(1.0, 0.0, 1.0))
            .normalize_or_zero()
            * 0.25
            + Vec3::Y * -0.08;

        let target_direction = target_transform.rotation.mul_vec3(Vec3::Z);
        let mut target_translation =
            player_head + head_offset * distance.clamp(0.0, 3.0) + target_direction * distance;

        if target_translation.y < 0.1 {
            distance -= (target_translation.y - 0.1) / target_direction.y;
            target_translation =
                player_head + head_offset * distance.clamp(0.0, 3.0) + target_direction * distance;
        }

        if distance > 0.0 {
            // cast to check visibility
            let scenes_head = containing_scene.get_position(player_head);
            let scenes_cam =
                containing_scene.get_position(player_head + target_direction * distance);

            const OFFSET_SIZE: f32 = 0.15;
            let offsets = [
                Vec3::ZERO,
                Vec3::new(-OFFSET_SIZE, 0.0, 0.0),
                Vec3::new(OFFSET_SIZE, 0.0, 0.0),
                Vec3::new(0.0, -OFFSET_SIZE, 0.0),
                Vec3::new(0.0, OFFSET_SIZE, 0.0),
            ];
            let mut offset_distances = [FloatOrd(1.0); 5];
            for scene in (scenes_head).union(&scenes_cam) {
                let Ok((context, mut colliders)) = scene_colliders.get_mut(*scene) else {
                    continue;
                };

                for ix in 0..5 {
                    let origin = player_head + target_transform.rotation.mul_vec3(offsets[ix]);
                    if let Some(hit) = colliders.cast_ray_nearest(
                        context.last_update_frame,
                        origin,
                        target_translation - origin,
                        1.0,
                        u32::MAX,
                        false,
                    ) {
                        offset_distances[ix] =
                            FloatOrd(offset_distances[ix].0.min(hit.toi).max(0.0));
                    }
                }
            }
            debug!(
                "{distance} vs {:?}",
                offset_distances.iter().map(|d| d.0).collect::<Vec<_>>()
            );
            distance *= offset_distances.iter().max().unwrap().0;
        }

        target_transform.translation =
            player_head + head_offset * distance.clamp(0.0, 3.0) + target_direction * distance;
    }

    let changed = (prev_override.is_some() != options.scene_override.is_some())
        || prev_override
            .as_ref()
            .is_some_and(|prev| !prev.effectively_equals(options.scene_override.as_ref().unwrap()))
        || *prev_oow != is_oow;

    if changed {
        debug!("changed cam to {:?}", options.scene_override);
        prev_override.clone_from(&options.scene_override);
        *prev_oow = is_oow;
        let time = match target_transition {
            TransitionMode::Time(t) => t,
            TransitionMode::Speed(s) => {
                let distance =
                    (target_transform.translation - camera_transform.translation).length();
                distance / s.max(0.001)
            }
        };
        debug!(
            "tween {:?} to {:?} over {time} seconds",
            camera_transform, target_transform
        );
        commands.entity(camera_ent).try_insert(SystemTween {
            target: target_transform,
            time,
        });
    } else if let Some(mut tween) = maybe_tween {
        if target_transform != tween.bypass_change_detection().target {
            debug!(
                "tween changed to {:?} to {:?}",
                camera_transform, target_transform
            );
            tween.bypass_change_detection().target = target_transform;
        }
    } else {
        commands
            .entity(camera_ent)
            .modify_component(move |t: &mut Transform| *t = target_transform);
        // *camera_transform = target_transform;
    }
}

pub fn update_cursor_lock(
    locks: Res<CursorLocks>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut prev: Local<bool>,
) {
    let lock = !locks.0.is_empty();

    if lock {
        for mut window in &mut windows {
            if !window.focused {
                if !*prev {
                    // new right-click while not focussed - try to focus
                    window.focused = true;
                    // and skip updating window fields until focus is processed
                    return;
                } else {
                    continue;
                }
            }

            if window.cursor_options.grab_mode == CursorGrabMode::None {
                window.cursor_options.grab_mode = CursorGrabMode::Locked;
                return;
            }

            if window.cursor_options.visible {
                window.cursor_options.visible = false;
            }
        }
    } else {
        for mut window in &mut windows {
            if window.cursor_options.grab_mode != CursorGrabMode::None {
                window.cursor_options.grab_mode = CursorGrabMode::None;
            }

            if !window.cursor_options.visible {
                window.cursor_options.visible = true;
            }
        }
    }

    *prev = lock;
}
