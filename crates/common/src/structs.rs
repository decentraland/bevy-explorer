use bevy::prelude::*;
use serde::{Deserialize, Serialize};

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

#[derive(Resource)]
pub struct PrimaryCameraRes(pub Entity);

// marker for the root ui component (full screen, used for checking pointer/mouse button events are not intercepted by any other ui component)
#[derive(Component)]
pub struct UiRoot;

// app configuration
#[derive(Serialize, Deserialize, Resource)]
pub struct AppConfig {
    pub server: String,
    pub profile_version: u32,
    pub profile_content: String,
    pub profile_base_url: String,
    pub graphics: GraphicsSettings,
    pub scene_threads: usize,
    pub scene_loop_millis: u64,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: "https://sdk-team-cdn.decentraland.org/ipfs/goerli-plaza-main".to_owned(),
            profile_version: 1,
            profile_content: Default::default(),
            profile_base_url: "https://peer.decentraland.zone/content/contents/".to_owned(),
            graphics: Default::default(),
            scene_threads: 4,
            scene_loop_millis: 12, // ~80fps
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct GraphicsSettings {
    pub vsync: bool,
    pub log_fps: bool,
    pub msaa: usize,
}

impl Default for GraphicsSettings {
    fn default() -> Self {
        Self {
            vsync: false,
            log_fps: true,
            msaa: 4,
        }
    }
}
