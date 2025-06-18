use std::marker::PhantomData;

use bevy::{ecs::system::SystemParam, prelude::*, window::PrimaryWindow};
use common::{
    inputs::{Action, SystemAction, POINTER_SET},
    structs::{ActiveDialog, AppConfig, CursorLocks, PrimaryCamera},
};
use input_manager::{InputManager, InputPriority};

use crate::{
    initialize_scene::SuperUserScene, renderer_context::RendererSceneContext,
    update_world::AddCrdtInterfaceExt, SceneSets,
};
use dcl::interface::{ComponentPosition, CrdtType};
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
        app.add_crdt_lww_component::<PbPointerLock, PointerLock>(
            SceneComponentId::POINTER_LOCK,
            ComponentPosition::RootOnly,
        );
        app.add_systems(Update, update_pointer_lock.in_set(SceneSets::Input));
    }
}

#[derive(Component)]
pub struct CumulativePointerDelta {
    pub delta: Vec2,
    pub since: f32,
}

#[derive(Component)]
pub struct PointerLock(PbPointerLock);

impl From<PbPointerLock> for PointerLock {
    fn from(value: PbPointerLock) -> Self {
        Self(value)
    }
}

#[derive(SystemParam)]
pub struct CameraInteractionState<'w, 's> {
    input_manager: InputManager<'w>,
    state: Local<'s, (ClickState, f32)>,
    time: Res<'w, Time>,
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

impl CameraInteractionState<'_, '_> {
    pub fn update(&mut self, action: Action) -> ClickState {
        match self.state.0 {
            ClickState::None | ClickState::Released => {
                if self.input_manager.just_down(action, InputPriority::None) {
                    *self.state = (ClickState::Held, self.time.elapsed_secs());
                } else {
                    self.state.0 = ClickState::None;
                }
            }
            ClickState::Held => {
                if self.input_manager.just_up(action) {
                    if self.time.elapsed_secs() - self.state.1 > 0.25 {
                        self.state.0 = ClickState::Released;
                    } else {
                        self.state.0 = ClickState::Clicked;
                    }
                }
            }
            ClickState::Clicked => self.state.0 = ClickState::Released,
        }

        self.state.0
    }
}

pub fn update_pointer_lock(
    mut commands: Commands,
    mut scenes: Query<(
        Entity,
        &mut RendererSceneContext,
        Option<&mut CumulativePointerDelta>,
        Option<&SuperUserScene>,
        Option<Ref<PointerLock>>,
    )>,
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<PrimaryCamera>>,
    mut prev_coords: Local<Option<Vec2>>,
    mut locks: ResMut<CursorLocks>,
    config: Res<AppConfig>,
    mut mb_state: CameraInteractionState,
    active_dialog: Option<Res<ActiveDialog>>,
    mut toggle: Local<bool>,
    mut last_explicit_tick: Local<u32>,
) {
    let Ok(window) = window.single() else {
        return;
    };
    let Ok((camera, camera_position)) = camera.single() else {
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

        if window.cursor_options.grab_mode == bevy::window::CursorGrabMode::Locked {
            // if pointer locked, just middle
            let window_size = Vec2::new(right - left, bottom - top);
            Some(window_size / 2.0)
        } else {
            let window_origin = Vec2::new(left, top);
            window.cursor_position().map(|cp| cp - window_origin)
        }
    };
    *prev_coords = screen_coordinates;

    // Handle mouse input
    let mut state = mb_state.update(Action::System(SystemAction::CameraLock));
    let input_manager = &mb_state.input_manager;
    if state == ClickState::None
        && input_manager.just_down(SystemAction::Cancel, InputPriority::None)
        && *toggle
    {
        // override
        state = ClickState::Released;
        *toggle = false;
    }

    if state == ClickState::Clicked {
        *toggle = !*toggle;
    }

    let mut camera_locked =
        active_dialog.is_none_or(|ad| !ad.in_use()) && (state == ClickState::Held || *toggle);

    for (_, context, _, maybe_super, maybe_lock) in scenes.iter_mut() {
        if maybe_super.is_some()
            && maybe_lock
                .as_ref()
                .is_some_and(|lock| lock.is_changed() || context.tick_number == *last_explicit_tick)
        {
            debug!("lock updated by scene");
            *toggle = maybe_lock.unwrap().0.is_pointer_locked;
            camera_locked = *toggle;
            *last_explicit_tick = context.tick_number;
        }
    }

    if camera_locked {
        locks.0.insert("camera");
    } else {
        locks.0.remove("camera");
    }

    let pointer_lock = PbPointerLock {
        is_pointer_locked: camera_locked,
    };

    let frame_delta = input_manager.get_analog(POINTER_SET, InputPriority::Scene);

    let ray = screen_coordinates
        .and_then(|coords| camera.viewport_to_world(camera_position, coords).ok())
        .map(|ray| Vector3::world_vec_from_vec3(&ray.direction));

    for (entity, mut context, maybe_pointer_delta, _, _) in scenes.iter_mut() {
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
