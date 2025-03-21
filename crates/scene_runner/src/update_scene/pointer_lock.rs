use bevy::{
    input::mouse::MouseMotion,
    prelude::*,
    window::{CursorGrabMode, PrimaryWindow},
};
use common::structs::{AppConfig, CursorLocks, PrimaryCamera};

use crate::{renderer_context::RendererSceneContext, SceneSets};
use dcl::interface::CrdtType;
use dcl_component::{
    proto_components::{
        common::Vector3,
        sdk::components::{PbPointerLock, PbPrimaryPointerInfo, PointerType},
    },
    SceneComponentId, SceneEntityId,
};

pub struct PointerLockPlugin;

impl Plugin for PointerLockPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, update_pointer_lock.in_set(SceneSets::Input));
    }
}

#[derive(Component)]
pub struct CumulativePointerDelta {
    pub delta: Vec2,
    pub since: f32,
}

fn update_pointer_lock(
    mut commands: Commands,
    mut scenes: Query<(
        Entity,
        &mut RendererSceneContext,
        Option<&mut CumulativePointerDelta>,
    )>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut mouse_events: EventReader<MouseMotion>,
    camera: Query<(&Camera, &GlobalTransform), With<PrimaryCamera>>,
    mut prev_coords: Local<Option<Vec2>>,
    locks: Res<CursorLocks>,
    config: Res<AppConfig>,
) {
    let Ok(window) = window.get_single() else {
        return;
    };
    let Ok((camera, camera_position)) = camera.get_single() else {
        return;
    };

    let screen_coordinates = if locks.0.contains("pointer") {
        *prev_coords
    } else {
        let real_window_size = Vec2::new(window.width(), window.height());
        let vmin = real_window_size.min_element();
        let (left, top, right, bottom) = if config.constrain_scene_ui {
            (
                vmin * 0.27,
                vmin * 0.06,
                real_window_size.x - vmin * 0.12,
                real_window_size.y - vmin * 0.06,
            )
        } else {
            (0.0, 0.0, real_window_size.x, real_window_size.y)
        };

        if window.cursor.grab_mode == bevy::window::CursorGrabMode::Locked {
            // if pointer locked, just middle
            let window_size = Vec2::new(right - left, bottom - top);
            Some(window_size / 2.0)
        } else {
            let window_origin = Vec2::new(left, top);
            window.cursor_position().map(|cp| cp - window_origin)
        }
    };
    *prev_coords = screen_coordinates;

    let pointer_lock = PbPointerLock {
        is_pointer_locked: window.cursor.grab_mode == CursorGrabMode::Locked,
    };

    let mut frame_delta = Vec2::ZERO;
    for mouse_event in mouse_events.read() {
        frame_delta += mouse_event.delta;
    }

    let ray = screen_coordinates
        .and_then(|coords| camera.viewport_to_world(camera_position, coords))
        .map(|ray| Vector3::world_vec_from_vec3(&ray.direction));

    for (entity, mut context, maybe_pointer_delta) in scenes.iter_mut() {
        if let Some(mut pointer_delta) = maybe_pointer_delta {
            if context.last_sent == pointer_delta.since {
                pointer_delta.delta += frame_delta;
            } else {
                pointer_delta.delta = frame_delta;
                pointer_delta.since = context.last_sent;
            };

            let pointer_info = PbPrimaryPointerInfo {
                pointer_type: Some(PointerType::PotMouse as i32),
                screen_coordinates: screen_coordinates.map(Into::into),
                screen_delta: Some(pointer_delta.delta.into()),
                world_ray_direction: ray,
            };

            context.update_crdt(
                SceneComponentId::PRIMARY_POINTER_INFO,
                CrdtType::LWW_ENT,
                SceneEntityId::ROOT,
                &pointer_info,
            );
        } else {
            commands.entity(entity).try_insert(CumulativePointerDelta {
                delta: frame_delta,
                since: context.last_sent,
            });
        }
        context.update_crdt(
            SceneComponentId::POINTER_LOCK,
            CrdtType::LWW_ENT,
            SceneEntityId::CAMERA,
            &pointer_lock,
        );
    }
}
