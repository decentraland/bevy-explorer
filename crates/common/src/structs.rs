use std::{f32::consts::PI, num::ParseIntError, ops::Range, str::FromStr, sync::Arc};

use bevy::{
    platform::collections::{HashMap, HashSet},
    prelude::*,
    render::view::RenderLayers,
};
use dcl_component::proto_components::sdk::components::common::CameraTransition;
use ethers_core::abi::Address;
use serde::{Deserialize, Serialize};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::inputs::InputMapSerialized;

#[derive(Resource)]
pub struct Version(pub String);

// main user entity
#[derive(Component, Clone, Serialize, Deserialize)]
pub struct PrimaryUser {
    pub walk_speed: f32,
    pub run_speed: f32,
    pub friction: f32,
    pub gravity: f32,
    pub jump_height: f32,
    pub fall_speed: f32,
    pub control_type: AvatarControl,
    pub turn_speed: f32,
    pub block_run: bool,
    pub block_walk: bool,
    pub block_jump: bool,
    pub block_emote: bool,
}

impl Default for PrimaryUser {
    fn default() -> Self {
        Self {
            walk_speed: 2.5,
            run_speed: 8.0,
            friction: 6.0,
            gravity: -10.0,
            jump_height: 1.25,
            fall_speed: -15.0,
            control_type: AvatarControl::Relative,
            turn_speed: PI,
            block_run: false,
            block_walk: false,
            block_jump: false,
            block_emote: false,
        }
    }
}

#[derive(Component, Default)]
pub struct PlayerModifiers {
    pub hide: bool,
    pub hide_profile: bool,
    pub walk_speed: Option<f32>,
    pub run_speed: Option<f32>,
    pub friction: Option<f32>,
    pub gravity: Option<f32>,
    pub jump_height: Option<f32>,
    pub fall_speed: Option<f32>,
    pub control_type: Option<AvatarControl>,
    pub turn_speed: Option<f32>,
    pub block_run: bool,
    pub block_walk: bool,
    pub block_jump: bool,
    pub block_emote: bool,
    pub areas: Vec<ActiveAvatarArea>,
}

#[derive(Clone)]
pub struct ActiveAvatarArea {
    pub entity: Entity,
    pub allow_locomotion: PermissionState,
}

impl PlayerModifiers {
    pub fn combine(&self, user: &PrimaryUser) -> PrimaryUser {
        PrimaryUser {
            walk_speed: self.walk_speed.unwrap_or(user.walk_speed),
            run_speed: self.run_speed.unwrap_or(user.run_speed),
            friction: self.friction.unwrap_or(user.friction),
            gravity: self.gravity.unwrap_or(user.gravity),
            jump_height: self.jump_height.unwrap_or(user.jump_height),
            fall_speed: self.fall_speed.unwrap_or(user.fall_speed),
            control_type: self.control_type.unwrap_or(user.control_type),
            turn_speed: self.turn_speed.unwrap_or(user.turn_speed),
            block_run: self.block_run || user.block_run,
            block_walk: self.block_walk || user.block_walk,
            block_jump: self.block_jump || user.block_jump,
            block_emote: self.block_emote || user.block_emote,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PermissionState {
    Resolved(bool),
    NotRequested,
    Pending,
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
                .spawn((Transform::default(), Visibility::default()))
                .id(),
            nametag: commands
                .spawn((
                    Transform::from_translation(Vec3::Y * 2.2),
                    Visibility::default(),
                ))
                .id(),
            left_hand: commands
                .spawn((
                    Transform::from_rotation(Quat::from_rotation_y(PI)),
                    Visibility::default(),
                ))
                .id(),
            right_hand: commands
                .spawn((
                    Transform::from_rotation(Quat::from_rotation_y(PI)),
                    Visibility::default(),
                ))
                .id(),
        }
    }

    pub fn entities(&self) -> [Entity; 4] {
        [self.position, self.nametag, self.left_hand, self.right_hand]
    }
}

#[derive(Component, Clone, Debug, PartialEq, Default)]
pub struct EmoteCommand {
    pub urn: String,
    pub timestamp: i64,
    pub r#loop: bool,
}

// main camera entity
#[derive(Component)]
pub struct PrimaryCamera {
    // settings
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

#[derive(Clone, Debug)]
pub struct CinematicSettings {
    pub origin: Entity,
    pub allow_manual_rotation: bool,
    pub yaw_range: Option<f32>,
    pub pitch_range: Option<f32>,
    pub roll_range: Option<f32>,
    pub zoom_min: Option<f32>,
    pub zoom_max: Option<f32>,
    pub look_at_entity: Option<Entity>,
    pub transition: Option<CameraTransition>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum AvatarControl {
    None,
    Relative,
    Tank,
}

#[derive(Clone, Debug)]
pub enum CameraOverride {
    Distance(f32),
    Cinematic(CinematicSettings),
}

impl CameraOverride {
    pub fn effectively_equals(&self, other: &CameraOverride) -> bool {
        match (self, other) {
            (CameraOverride::Distance(x), CameraOverride::Distance(y)) => x == y,
            (CameraOverride::Cinematic(c0), CameraOverride::Cinematic(c1)) => {
                c0.origin == c1.origin
            }
            _ => false,
        }
    }
}

impl Default for PrimaryCamera {
    fn default() -> Self {
        Self {
            sensitivity: 5.0,
            initialized: Default::default(),
            yaw: Default::default(),
            pitch: Default::default(),
            roll: Default::default(),
            distance: 1.0,
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

#[derive(PartialEq, Eq, Hash, PartialOrd, Ord, Clone, Copy)]
pub enum TooltipSource {
    Label(&'static str),
    Entity(Entity),
}

#[derive(Resource, Default)]
pub struct ToolTips(pub HashMap<TooltipSource, Vec<(String, bool)>>);

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
#[serde(default)]
pub struct AppConfig {
    pub server: String,
    pub location: IVec2,
    pub previous_login: Option<PreviousLogin>,
    pub graphics: GraphicsSettings,
    pub audio: AudioSettings,
    pub cache_bytes: u64,
    pub scene_threads: usize,
    pub scene_load_distance: f32,
    pub scene_unload_extra_distance: f32,
    pub scene_imposter_distances: Vec<f32>,
    pub scene_imposter_multisample: bool,
    pub scene_imposter_multisample_amount: f32,
    pub scene_imposter_bake: SceneImposterBake,
    pub sysinfo_visible: bool,
    pub scene_log_to_console: bool,
    pub max_avatars: usize,
    pub constrain_scene_ui: bool,
    pub player_settings: PrimaryUser,
    pub max_videos: usize,
    pub max_concurrent_remotes: usize,
    pub user_id: String,
    pub default_permissions: HashMap<PermissionType, PermissionValue>,
    pub realm_permissions: HashMap<String, HashMap<PermissionType, PermissionValue>>,
    pub scene_permissions: HashMap<String, HashMap<PermissionType, PermissionValue>>,
    pub inputs: InputMapSerialized,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: "https://realm-provider-ea.decentraland.org/main".to_owned(),
            location: IVec2::new(0, 0),
            previous_login: None,
            graphics: Default::default(),
            audio: Default::default(),
            cache_bytes: 1024 * 1024 * 1024 * 10, // 10gb
            scene_threads: 4,
            scene_load_distance: 50.0,
            scene_unload_extra_distance: 15.0,
            scene_imposter_distances: vec![0.0],
            // scene_imposter_distances: vec![150.0, 300.0, 600.0, 1200.0, 2400.0, 4800.0],
            scene_imposter_multisample: false,
            scene_imposter_multisample_amount: 0.0,
            scene_imposter_bake: SceneImposterBake::Off,
            sysinfo_visible: false,
            scene_log_to_console: false,
            max_avatars: 100,
            constrain_scene_ui: false,
            player_settings: Default::default(),
            max_videos: 1,
            max_concurrent_remotes: 32,
            user_id: uuid::Uuid::new_v4().to_string(),
            default_permissions: Default::default(),
            realm_permissions: Default::default(),
            scene_permissions: Default::default(),
            inputs: Default::default(),
        }
    }
}

impl AppConfig {
    pub fn get_permission(
        &self,
        ty: PermissionType,
        realm: impl AsRef<str>,
        scene: impl AsRef<str>,
        is_portable: bool,
    ) -> PermissionValue {
        self.scene_permissions
            .get(scene.as_ref())
            .and_then(|map| map.get(&ty))
            .or_else(|| {
                if !is_portable {
                    self.realm_permissions
                        .get(realm.as_ref())
                        .and_then(|map| map.get(&ty))
                } else {
                    None
                }
            })
            .or_else(|| self.default_permissions.get(&ty))
            .copied()
            .unwrap_or_else(|| Self::default_permission(ty))
    }

    pub const fn default_permission(ty: PermissionType) -> PermissionValue {
        match ty {
            PermissionType::MovePlayer
            | PermissionType::ForceCamera
            | PermissionType::PlayEmote
            | PermissionType::SetLocomotion
            | PermissionType::HideAvatars
            | PermissionType::DisableVoice => PermissionValue::Allow,
            _ => PermissionValue::Ask,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct GraphicsSettings {
    pub vsync: bool,
    pub log_fps: bool,
    pub msaa: AaSetting,
    pub fps_target: usize,
    pub shadow_distance: f32,
    pub shadow_settings: ShadowSetting,
    pub shadow_caster_count: usize,
    pub window: WindowSetting,
    // removed until bevy window resizing bugs are fixed
    // pub fullscreen_res: FullscreenResSetting,
    pub fog: FogSetting,
    pub bloom: BloomSetting,
    pub dof: DofSetting,
    pub ssao: SsaoSetting,
    pub oob: f32,
    pub ambient_brightness: i32,
    pub gpu_bytes_per_frame: usize,
}

impl Default for GraphicsSettings {
    fn default() -> Self {
        Self {
            vsync: false,
            log_fps: true,
            msaa: AaSetting::FxaaHigh,
            fps_target: 60,
            shadow_distance: 200.0,
            shadow_settings: ShadowSetting::High,
            shadow_caster_count: 8,
            window: WindowSetting::Windowed,
            // fullscreen_res: FullscreenResSetting(UVec2::new(1280,720)),
            fog: FogSetting::Atmospheric,
            bloom: BloomSetting::Low,
            dof: DofSetting::High,
            ssao: SsaoSetting::Off,
            oob: 2.0,
            ambient_brightness: 50,
            gpu_bytes_per_frame: 0,
        }
    }
}

#[derive(Resource, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AudioSettings {
    pub master: i32, // 0-100
    pub voice: i32,
    pub scene: i32,
    pub system: i32,
    pub avatar: i32,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            master: 100,
            voice: 100,
            scene: 100,
            system: 100,
            avatar: 100,
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
    pub fn avatar(&self) -> f32 {
        (self.avatar * self.master) as f32 / 10_000.0
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

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum DofSetting {
    Off,
    Low,
    High,
}

#[derive(Component)]
// (sensor height, extra focal distance)
pub struct DofConfig {
    pub default_sensor_height: f32,
    pub extra_focal_distance: f32,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum SsaoSetting {
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
    pub load_imposter: f32,
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
    pub owner: Option<String>,
    pub display: Option<SceneDisplay>,
    pub main: String,
    pub scene: SceneMetaScene,
    pub runtime_version: Option<String>,
    pub spawn_points: Option<Vec<SpawnPoint>>,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PermissionValue {
    Allow,
    Deny,
    Ask,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PermissionType {
    MovePlayer,
    ForceCamera,
    PlayEmote,
    SetLocomotion,
    HideAvatars,
    DisableVoice,
    Teleport,
    ChangeRealm,
    SpawnPortable,
    KillPortables,
    Web3,
    CopyToClipboard,
    Fetch,
    Websocket,
    OpenUrl,
}

#[derive(Resource)]
pub struct ActiveDialog(Arc<Semaphore>);

impl Default for ActiveDialog {
    fn default() -> Self {
        Self(Arc::new(Semaphore::new(1)))
    }
}

impl ActiveDialog {
    pub fn try_acquire(&self) -> Option<DialogPermit> {
        self.0
            .clone()
            .try_acquire_owned()
            .ok()
            .map(|p| DialogPermit { _p: Some(p) })
    }

    pub fn in_use(&self) -> bool {
        self.0.available_permits() == 0
    }
}

#[derive(Component)]
pub struct DialogPermit {
    _p: Option<OwnedSemaphorePermit>,
}

impl DialogPermit {
    pub fn take(&mut self) -> Self {
        Self {
            _p: Some(self._p.take().unwrap()),
        }
    }
}

#[derive(Component, Default, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    ProfileDetail,
    #[default]
    Wearables,
    Emotes,
    Map,
    Discover,
    Settings,
    Permissions,
}

#[derive(Event, Clone)]
pub struct ShowSettingsEvent(pub SettingsTab);

#[derive(Event, Clone)]
pub struct ShowProfileEvent(pub Address);

#[derive(Event, Clone)]
pub struct SystemAudio(pub String);

impl From<&str> for SystemAudio {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl From<String> for SystemAudio {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&String> for SystemAudio {
    fn from(value: &String) -> Self {
        Self(value.to_owned())
    }
}

#[derive(Resource, Default)]
pub struct PermissionTarget {
    pub scene: Option<Entity>,
    pub ty: Option<PermissionType>,
}

// render layers
// 0 is default
// - normally 0 and 1 is used for the player, when in first person only 1 is used for the player
// - world lights target both 0 and 1, the main camera uses 0
// - this allows shadows to be cast by the player without the player being visible
pub const PRIMARY_AVATAR_LIGHT_LAYER_INDEX: usize = 1;
pub const PRIMARY_AVATAR_LIGHT_LAYER: RenderLayers =
    RenderLayers::layer(PRIMARY_AVATAR_LIGHT_LAYER_INDEX);
// layer for profile content
pub const PROFILE_UI_RENDERLAYER: RenderLayers = RenderLayers::layer(3);
// layer for ground
pub const GROUND_RENDERLAYER: RenderLayers = RenderLayers::layer(4);

#[derive(PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum SceneImposterBake {
    Off,
    FullSpeed,
    HalfSpeed,
    QuarterSpeed,
}

impl SceneImposterBake {
    pub fn as_mult(&self) -> f32 {
        match self {
            SceneImposterBake::Off => panic!(),
            SceneImposterBake::FullSpeed => 1.0,
            SceneImposterBake::HalfSpeed => 0.5,
            SceneImposterBake::QuarterSpeed => 0.25,
        }
    }
}

#[derive(Resource, Default)]
pub struct CursorLocks(pub HashSet<&'static str>);

#[derive(Default, Clone, Copy)]
pub enum MoveKind {
    #[default]
    Idle,
    Walk,
    Jog,
    Run,
    Jump,
    Falling,
    LongFalling,
    Emote,
}

#[derive(Component, Default)]
pub struct AvatarDynamicState {
    pub force: Vec2,
    pub velocity: Vec3,
    pub ground_height: f32,
    pub tank: bool,
    pub rotate: f32,
    pub jump_time: f32,
    pub move_kind: MoveKind,
}

#[derive(Event)]
pub enum PreviewCommand {
    ReloadScene { hash: String },
}

#[derive(Resource)]
pub struct SystemScene {
    pub source: Option<String>,
    pub preview: bool,
    pub hot_reload: Option<tokio::sync::mpsc::UnboundedSender<PreviewCommand>>,
    pub hash: Option<String>,
}

#[derive(Resource, Default, Clone, Debug)]
pub struct SceneGlobalLight {
    pub source: Option<Entity>,
    pub dir_color: Color,
    pub dir_illuminance: f32,
    pub dir_direction: Vec3,
    pub ambient_color: Color,
    pub ambient_brightness: f32,
    pub layers: RenderLayers,
}

#[derive(Resource)]
pub struct TimeOfDay {
    pub time: f32, // secs since midnight
    pub target_time: Option<f32>,
    pub speed: f32,
}

impl TimeOfDay {
    pub fn elapsed_secs(&self) -> f32 {
        self.time
    }
}

// porting aid, used to be one component
pub type TextStyle = (TextFont, TextColor);

// non-spatial audio
#[derive(Component)]
pub struct AudioEmitter {
    pub instances: Vec<Handle<bevy_kira_audio::AudioInstance>>,
}

#[derive(Clone, Copy)]
#[repr(i32)]
pub enum ZOrder {
    Crosshair = -65536,
    // PortableScene -> -65535 <= value <= -1
    // default 0 => appear in world, under scene ui
    SceneUi = 1,
    SceneUiOverlay,
    SystemSceneUi,
    SystemSceneUiOverlay,
    MouseInteractionComponent,
    SceneLoadingDialog,
    ChatBubble,
    NftDialog,
    EmoteSelect,
    ProfileView,
    Login,
    Minimap,
    SystemUi,
    ToolTip,
    Backpack,
    BackpackPopup,
    Toast,
    Permission,
    DefaultComboPopup,
    EguiBlocker,
}

impl ZOrder {
    pub fn default(self) -> GlobalZIndex {
        GlobalZIndex(self as i32)
    }
}
