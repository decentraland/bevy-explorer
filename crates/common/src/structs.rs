use std::{f32::consts::PI, num::ParseIntError, ops::Range, str::FromStr};

use bevy::{prelude::*, utils::HashMap};
use ethers_core::abi::Address;
use serde::{Deserialize, Serialize};

#[derive(Resource)]
pub struct Version(pub String);

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
            position: commands.spawn(SpatialBundle::default()).id(),
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

// component holding avatar texture (just the face currently)
#[derive(Component, Default)]
pub struct AvatarTextureHandle(pub Handle<Image>);

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
            keyboard_key_enable_mouse: KeyCode::KeyM,
            sensitivity: 5.0,
            initialized: Default::default(),
            yaw: Default::default(),
            pitch: Default::default(),
            roll: Default::default(),
            distance: 1.0,
            key_roll_left: KeyCode::KeyT,
            key_roll_right: KeyCode::KeyG,
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
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChainLink {
    #[serde(rename = "type")]
    pub ty: String,
    pub payload: String,
    pub signature: String,
}

// ephemeral identity info
#[derive(Serialize, Deserialize, Clone)]
pub struct PreviousLogin {
    pub root_address: Address,
    pub ephemeral_key: Vec<u8>,
    pub auth: Vec<ChainLink>,
}

// app configuration
#[derive(Serialize, Deserialize, Resource, Clone)]
pub struct AppConfig {
    pub server: String,
    pub location: IVec2,
    pub previous_login: Option<PreviousLogin>,
    pub graphics: GraphicsSettings,
    pub audio: AudioSettings,
    pub scene_threads: usize,
    pub scene_load_distance: f32,
    pub scene_unload_extra_distance: f32,
    pub sysinfo_visible: bool,
    pub scene_log_to_console: bool,
    pub max_avatars: usize,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: "https://sdk-team-cdn.decentraland.org/ipfs/goerli-plaza-update-asset-pack-lib"
                .to_owned(),
            location: IVec2::new(78, -7),
            previous_login: None,
            graphics: Default::default(),
            audio: Default::default(),
            scene_threads: 4,
            scene_load_distance: 75.0,
            scene_unload_extra_distance: 25.0,
            sysinfo_visible: true,
            scene_log_to_console: false,
            max_avatars: 100,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GraphicsSettings {
    pub vsync: bool,
    pub log_fps: bool,
    pub msaa: AaSetting,
    pub fps_target: usize,
    pub shadow_distance: f32,
    pub shadow_settings: ShadowSetting,
    pub window: WindowSetting,
    // removed until bevy window resizing bugs are fixed
    // pub fullscreen_res: FullscreenResSetting,
    pub fog: FogSetting,
    pub bloom: BloomSetting,
    pub oob: f32,
}

impl Default for GraphicsSettings {
    fn default() -> Self {
        Self {
            vsync: false,
            log_fps: true,
            msaa: AaSetting::Msaa4x,
            fps_target: 60,
            shadow_distance: 100.0,
            shadow_settings: ShadowSetting::High,
            window: WindowSetting::Windowed,
            // fullscreen_res: FullscreenResSetting(UVec2::new(1280,720)),
            fog: FogSetting::Atmospheric,
            bloom: BloomSetting::Low,
            oob: 2.0,
        }
    }
}

#[derive(Resource, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AudioSettings {
    pub master: i32, // 0-100
    pub voice: i32,
    pub scene: i32,
    pub system: i32,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            master: 100,
            voice: 100,
            scene: 100,
            system: 100,
        }
    }
}

impl AudioSettings {
    pub fn voice(&self) -> f32 {
        (self.voice * self.master) as f32 / 10_000.0
    }
    pub fn scene(&self) -> f32 {
        (self.scene * self.master) as f32 / 10_000.0
    }
    pub fn system(&self) -> f32 {
        (self.system * self.master) as f32 / 10_000.0
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShadowSetting {
    Off,
    Low,
    High,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum AaSetting {
    Off,
    FxaaLow,
    FxaaHigh,
    Msaa2x,
    Msaa4x,
    Msaa8x,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum WindowSetting {
    Fullscreen,
    Windowed,
    Borderless,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct FullscreenResSetting(pub UVec2);

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum FogSetting {
    Off,
    Basic,
    Atmospheric,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum BloomSetting {
    Off,
    Low,
    High,
}

#[derive(Debug)]
pub enum AudioDecoderError {
    StreamClosed,
    Other(String),
}

#[derive(Resource)]
pub struct SceneLoadDistance {
    pub load: f32,
    pub unload: f32, // additional
}

#[derive(Debug)]
pub struct IVec2Arg(pub IVec2);

impl FromStr for IVec2Arg {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut chars = s.chars().peekable();

        let skip = |chars: &mut std::iter::Peekable<std::str::Chars>, numeric: bool| {
            while numeric
                == chars
                    .peek()
                    .map_or(!numeric, |c| c.is_numeric() || *c == '-')
            {
                chars.next();
            }
        };

        let parse = |chars: &std::iter::Peekable<std::str::Chars>| {
            chars
                .clone()
                .take_while(|c| c.is_numeric() || *c == '-')
                .collect::<String>()
                .parse::<i32>()
        };

        skip(&mut chars, false);
        let x = parse(&chars)?;
        skip(&mut chars, true);
        skip(&mut chars, false);
        let y = parse(&chars)?;

        Ok(IVec2Arg(IVec2::new(x, y)))
    }
}

// scene metadata
#[derive(Deserialize, Debug, Clone)]
pub struct SpawnPosition {
    x: serde_json::Value,
    y: serde_json::Value,
    z: serde_json::Value,
}

impl SpawnPosition {
    pub fn bounding_box(&self) -> (Vec3, Vec3) {
        let parse_val = |v: &serde_json::Value| -> Option<Range<f32>> {
            if let Some(val) = v.as_f64() {
                Some(val as f32..val as f32)
            } else if let Some(array) = v.as_array() {
                if let Some(mut start) = array.first().and_then(|s| s.as_f64()) {
                    let mut end = array.get(1).and_then(|e| e.as_f64()).unwrap_or(start);
                    if end < start {
                        (start, end) = (end, start);
                    }
                    Some(start as f32..end as f32)
                } else {
                    None
                }
            } else {
                None
            }
        };

        let x = parse_val(&self.x).unwrap_or(0.0..16.0);
        let y = parse_val(&self.y).unwrap_or(0.0..0.0);
        let z = parse_val(&self.z).unwrap_or(0.0..16.0);

        (
            Vec3::new(x.start, y.start, z.start),
            Vec3::new(x.end, y.end, z.end),
        )
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct SpawnPoint {
    pub name: Option<String>,
    pub default: bool,
    pub position: SpawnPosition,
}

#[derive(Deserialize, Debug)]
pub struct SceneMetaScene {
    pub base: String,
    pub parcels: Vec<String>,
}

#[derive(Deserialize, Debug)]
pub struct SceneDisplay {
    pub title: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SceneMeta {
    pub display: Option<SceneDisplay>,
    pub main: String,
    pub scene: SceneMetaScene,
    pub runtime_version: Option<String>,
    pub spawn_points: Option<Vec<SpawnPoint>>,
}
