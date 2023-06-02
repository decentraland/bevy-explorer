use bevy::prelude::*;

// main user entity
#[derive(Component)]
pub struct PrimaryUser {
    pub walk_speed: f32,
    pub run_speed: f32,
    pub friction: f32,
}

impl Default for PrimaryUser {
    fn default() -> Self {
        Self {
            walk_speed: 10.0,
            run_speed: 40.0,
            friction: 500.0,
        }
    }
}

// main camera entity
#[derive(Component)]
pub struct PrimaryCamera {
    // settings
    pub mouse_key_enable_mouse: MouseButton,
    pub keyboard_key_enable_mouse: KeyCode,
    pub key_roll_left: KeyCode,
    pub key_roll_right: KeyCode,
    pub distance: f32,
    pub sensitivity: f32,
    // impl details (todo: move to separate private struct)
    pub initialized: bool,
    pub yaw: f32,
    pub pitch: f32,
    pub roll: f32,
}

impl Default for PrimaryCamera {
    fn default() -> Self {
        Self {
            mouse_key_enable_mouse: MouseButton::Right,
            keyboard_key_enable_mouse: KeyCode::M,
            sensitivity: 5.0,
            initialized: Default::default(),
            yaw: Default::default(),
            pitch: Default::default(),
            roll: Default::default(),
            distance: 1.0,
            key_roll_left: KeyCode::T,
            key_roll_right: KeyCode::G,
        }
    }
}

// marker for the root ui component (full screen, used for checking pointer/mouse button events are not intercepted by any other ui component)
#[derive(Component)]
pub struct UiRoot;

// common phsyical variables (currently only fall speed is used by both foreign avater dynamics and user dynamics...)
pub mod dynamics {
    use std::f32::consts::PI;

    pub const MAX_FALL_SPEED: f32 = 15.0;
    pub const GRAVITY: f32 = 20.0;
    pub const MAX_CLIMBABLE_INCLINE: f32 = 1.5 * PI / 4.0; // radians from up - equal to 60 degree incline
    pub const MAX_STEP_HEIGHT: f32 = 0.5;
    pub const MAX_JUMP_HEIGHT: f32 = 1.25;
    pub const PLAYER_GROUND_THRESHOLD: f32 = 0.05;

    pub const PLAYER_COLLIDER_RADIUS: f32 = 0.35;
    pub const PLAYER_COLLIDER_HEIGHT: f32 = 2.0;
    pub const PLAYER_COLLIDER_OVERLAP: f32 = 0.01;
}
