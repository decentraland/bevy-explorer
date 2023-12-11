use std::f32::consts::PI;

use bevy::{prelude::*, utils::HashMap};
use ethers_core::abi::Address;
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
            walk_speed: 12.0,
            run_speed: 50.0,
            friction: 500.0,
        }
    }
}

// attachment points for local or foreign players
#[derive(Component)]
pub struct AttachPoints {
    pub position: Entity,
    pub nametag: Entity,
    pub left_hand: Entity,
    pub right_hand: Entity,
}

impl AttachPoints {
    pub fn new(commands: &mut Commands) -> Self {
        Self {
            position: commands
                .spawn(SpatialBundle {
                    // TODO this is weird and must be wrong
                    transform: Transform::from_translation(Vec3::Y * -0.7),
                    ..default()
                })
                .id(),
            nametag: commands
                .spawn(SpatialBundle {
                    transform: Transform::from_translation(Vec3::Y * 2.2),
                    ..default()
                })
                .id(),
            left_hand: commands
                .spawn(SpatialBundle {
                    transform: Transform::from_rotation(Quat::from_rotation_y(PI)),
                    ..Default::default()
                })
                .id(),
            right_hand: commands
                .spawn(SpatialBundle {
                    transform: Transform::from_rotation(Quat::from_rotation_y(PI)),
                    ..Default::default()
                })
                .id(),
        }
    }

    pub fn entities(&self) -> [Entity; 4] {
        [self.position, self.nametag, self.left_hand, self.right_hand]
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
    // override
    pub scene_override: Option<CameraOverride>,
}

pub enum CameraOverride {
    Distance(f32),
    Cinematic(Transform),
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
            scene_override: None,
        }
    }
}

#[derive(Resource)]
pub struct PrimaryPlayerRes(pub Entity);

#[derive(Resource)]
pub struct PrimaryCameraRes(pub Entity);

// marker for the root ui component (full screen, used for checking pointer/mouse button events are not intercepted by any other ui component)
#[derive(Component)]
pub struct UiRoot;

#[derive(Resource, Default)]
pub struct ToolTips(pub HashMap<&'static str, Vec<(String, bool)>>);

// web3 authorization chain link
#[derive(Serialize, Deserialize, Clone)]
pub struct ChainLink {
    #[serde(rename = "type")]
    pub ty: String,
    pub payload: String,
    pub signature: String,
}

// ephemeral identity info
#[derive(Serialize, Deserialize)]
pub struct PreviousLogin {
    pub root_address: Address,
    pub ephemeral_key: Vec<u8>,
    pub auth: Vec<ChainLink>,
}

// app configuration
#[derive(Serialize, Deserialize, Resource)]
pub struct AppConfig {
    pub server: String,
    pub location: IVec2,
    pub previous_login: Option<PreviousLogin>,
    pub graphics: GraphicsSettings,
    pub scene_threads: usize,
    pub scene_load_distance: f32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: "https://sdk-team-cdn.decentraland.org/ipfs/goerli-plaza-main".to_owned(),
            location: IVec2::new(78, -7),
            previous_login: None,
            graphics: Default::default(),
            scene_threads: 4,
            scene_load_distance: 100.0,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct GraphicsSettings {
    pub vsync: bool,
    pub log_fps: bool,
    pub msaa: usize,
    pub fps_target: usize,
}

impl Default for GraphicsSettings {
    fn default() -> Self {
        Self {
            vsync: false,
            log_fps: true,
            msaa: 4,
            fps_target: 60,
        }
    }
}

#[derive(Debug)]
pub enum AudioDecoderError {
    StreamClosed,
    Other(String),
}

#[derive(Resource)]
pub struct SceneLoadDistance(pub f32);
