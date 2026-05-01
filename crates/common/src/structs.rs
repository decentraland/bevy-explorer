use std::{
    f32::consts::PI,
    marker::PhantomData,
    num::ParseIntError,
    ops::{Deref, Range},
    str::FromStr,
    sync::{atomic::AtomicU32, Arc},
};

use bevy::{
    color::palettes,
    platform::collections::{HashMap, HashSet},
    prelude::*,
    render::{primitives::Aabb, view::RenderLayers},
};
use dcl_component::proto_components::sdk::components::common::CameraTransition;
use ethers_core::abi::Address;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::inputs::InputMapSerialized;

#[derive(Resource)]
pub struct Version(pub String);

// main user entity
#[derive(Component, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PrimaryUser {
    pub walk_speed: f32,
    pub jog_speed: f32,
    pub run_speed: f32,
    pub jump_height: f32,
    pub run_jump_height: f32,
    pub block_all: bool,
    pub block_run: bool,
    pub block_walk: bool,
    pub block_jump: bool,
    pub block_emote: bool,
}

impl Default for PrimaryUser {
    fn default() -> Self {
        Self {
            walk_speed: 2.5,
            jog_speed: 8.18,
            run_speed: 11.0,
            jump_height: 1.9,
            run_jump_height: 2.95,
            block_all: false,
            block_run: false,
            block_walk: false,
            block_jump: false,
            block_emote: false,
        }
    }
}

#[derive(Component, Default, Debug)]
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
    pub block_all: bool,
    pub block_run: bool,
    pub block_walk: bool,
    pub block_jump: bool,
    pub block_emote: bool,
    pub areas: Vec<ActiveAvatarArea>,
}

#[derive(Clone, Debug)]
pub struct ActiveAvatarArea {
    pub entity: Entity,
    pub allow_locomotion: PermissionState,
}

impl PlayerModifiers {
    pub fn combine(&self, user: &PrimaryUser) -> PrimaryUser {
        PrimaryUser {
            walk_speed: self.walk_speed.unwrap_or(user.walk_speed),
            jog_speed: self.run_speed.unwrap_or(user.jog_speed),
            run_speed: self.run_speed.unwrap_or(user.run_speed),
            jump_height: self.jump_height.unwrap_or(user.jump_height),
            run_jump_height: self.jump_height.unwrap_or(user.run_jump_height),
            block_all: self.block_all || user.block_all,
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
    pub head: Entity,
    pub neck: Entity,
    pub spine: Entity,
    pub spine_1: Entity,
    pub spine_2: Entity,
    pub hip: Entity,
    pub left_shoulder: Entity,
    pub left_arm: Entity,
    pub left_forearm: Entity,
    pub left_hand: Entity,
    pub left_hand_index: Entity,
    pub right_shoulder: Entity,
    pub righ_arm: Entity,
    pub right_forearm: Entity,
    pub right_hand: Entity,
    pub right_hand_index: Entity,
    /// AAPT_LEFT_UP_LEG
    pub left_thigh: Entity,
    /// AAPT_LEFT_LEG
    pub left_shin: Entity,
    pub left_foot: Entity,
    pub left_toe_base: Entity,
    /// AAPT_RIGHT_UP_LEG
    pub right_thigh: Entity,
    /// AAPT_RIGHT_LEG
    pub right_shin: Entity,
    pub right_foot: Entity,
    pub right_toe_base: Entity,
}

impl AttachPoints {
    pub fn new(commands: &mut Commands) -> Self {
        let inverted_transform = Transform::from_rotation(Quat::from_rotation_y(PI));
        let default_visibility = Visibility::default();
        let default_bundle = (inverted_transform, default_visibility);
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
            head: commands
                .spawn((
                    default_bundle,
                    // This Aabb roughly encloses the head
                    Aabb::from_min_max(Vec3::new(-16., -21., -22.), Vec3::new(16., 21., 22.)),
                ))
                .id(),
            neck: commands.spawn(default_bundle).id(),
            spine: commands.spawn(default_bundle).id(),
            spine_1: commands.spawn(default_bundle).id(),
            spine_2: commands.spawn(default_bundle).id(),
            hip: commands.spawn(default_bundle).id(),
            left_shoulder: commands.spawn(default_bundle).id(),
            left_arm: commands.spawn(default_bundle).id(),
            left_forearm: commands.spawn(default_bundle).id(),
            left_hand: commands.spawn(default_bundle).id(),
            left_hand_index: commands.spawn(default_bundle).id(),
            right_shoulder: commands.spawn(default_bundle).id(),
            righ_arm: commands.spawn(default_bundle).id(),
            right_forearm: commands.spawn(default_bundle).id(),
            right_hand: commands.spawn(default_bundle).id(),
            right_hand_index: commands.spawn(default_bundle).id(),
            left_thigh: commands.spawn(default_bundle).id(),
            left_shin: commands.spawn(default_bundle).id(),
            left_foot: commands.spawn(default_bundle).id(),
            left_toe_base: commands.spawn(default_bundle).id(),
            right_thigh: commands.spawn(default_bundle).id(),
            right_shin: commands.spawn(default_bundle).id(),
            right_foot: commands.spawn(default_bundle).id(),
            right_toe_base: commands.spawn(default_bundle).id(),
        }
    }

    /// AttachPoints entities ordered by their protocol id
    pub fn entities(&self) -> [Entity; 26] {
        [
            self.position,
            self.nametag,
            self.left_hand,
            self.right_hand,
            self.head,
            self.neck,
            self.spine,
            self.spine_1,
            self.spine_2,
            self.hip,
            self.left_shoulder,
            self.left_arm,
            self.left_forearm,
            self.left_hand_index,
            self.right_shoulder,
            self.righ_arm,
            self.right_forearm,
            self.right_hand_index,
            self.left_thigh,
            self.left_shin,
            self.left_foot,
            self.left_toe_base,
            self.right_thigh,
            self.right_shin,
            self.right_foot,
            self.right_toe_base,
        ]
    }
}

#[derive(Component, Clone, Debug, PartialEq, Default)]
pub struct EmoteCommand {
    pub urn: String,
    pub timestamp: i64,
    pub r#loop: bool,
}

// Current scene-driven movement animation request for a player avatar. For the
// primary player, written by the bridge system in `user_input` (after resolving
// the scene-relative path against the active scene's content map). For foreign
// players, written by the comms crate when a Movement packet with anim fields
// arrives. Read by `animate` in the avatar crate uniformly for both.
#[derive(Component, Default, Clone, Debug)]
pub struct SceneDrivenAnim {
    pub active: Option<SceneDrivenAnimationRequest>,
}

#[derive(Debug, Clone, Default)]
pub struct SceneDrivenAnimationRequest {
    // scene-relative path (e.g. "assets/walk.glb") — used for feedback to the controlling
    // scene. Empty for remote requests received over the network.
    pub src: String,
    // Pre-built scene-emote URN encoding scene_hash + content_hash. For local requests
    // this is constructed by user_input; for remote requests it's reassembled from the
    // hash pair received on the wire.
    pub urn: String,
    // The two hashes that compose the URN. Kept alongside `urn` so the broadcaster can
    // ship them to remotes without repeating the fixed URN preamble on every packet.
    pub scene_hash: String,
    pub content_hash: String,
    pub r#loop: bool,
    pub speed: f32,
    pub idle: bool,
    pub transition_seconds: f32,
    pub seek: Option<f32>,
    // Scene-requested avatar-bus sound clips to play this update. Each entry is the
    // content_hash of a file hosted in the same scene as the animation. The consumer
    // dedups per-avatar so leaving identical entries across consecutive updates doesn't
    // re-fire; the scene clears the list on frames it doesn't want sound.
    pub sounds: Vec<String>,
}

// Current scene-driven animation playback state, written by `play_current_emote` in
// the avatar crate and mirrored into `AvatarMovementInfo.active_animation_state` by
// `broadcast_movement_info` in `user_input`.
// `playback_time` freezes while a triggerSceneEmote overrides the animation.
#[derive(Resource, Default)]
pub struct SceneDrivenAnimationFeedback {
    pub state: Option<SceneDrivenAnimationFeedbackState>,
}

#[derive(Debug, Clone)]
pub struct SceneDrivenAnimationFeedbackState {
    pub src: String,
    pub r#loop: bool,
    pub speed: f32,
    pub idle: bool,
    pub playback_time: f32,
    pub duration: f32,
    pub loop_count: u32,
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

/// World-space head gaze angles, in degrees. On the local player, written each frame
/// from the real camera (and zeroed/disabled when a scene-driven camera is active). On
/// remote players, written from the incoming rfc4::Movement head-sync fields. The
/// `*_enabled` flags drive the receive-side IK weight crossfade and gate the broadcast
/// of valid angles for the local player.
#[derive(Component, Default, Clone, Copy)]
pub struct HeadSync {
    pub yaw_deg: f32,
    pub pitch_deg: f32,
    pub yaw_enabled: bool,
    pub pitch_enabled: bool,
}

/// Classifier for pointer-target hits — distinguishes scene world geometry,
/// scene UI overlays, and avatars. Lives here (not in system_bridge) because
/// the engine's pointer pipeline produces it before any scene API surface is
/// involved; system_bridge re-exports the same value into its HoverEvent.
#[derive(
    Hash,
    Clone,
    Copy,
    serde_repr::Serialize_repr,
    serde_repr::Deserialize_repr,
    Debug,
    PartialEq,
    Eq,
    Default,
)]
#[repr(u32)]
pub enum PointerTargetType {
    #[default]
    World = 0,
    Ui = 1,
    Avatar = 2,
}

/// World-space point-at state. On the local player, populated when the PointAt
/// action fires and `WorldPointerTarget` has a hit; cleared when the latch
/// expires. On remote players, populated from the incoming rfc4::Movement
/// `point_at_*` / `is_pointing_at` fields. Coordinates are stored in DCL
/// convention (Z mirrored from bevy world) so the same value flows over the
/// wire and into the IK apply step without a per-site flip.
#[derive(Component, Default, Clone, Copy)]
pub struct PointAtSync {
    pub target_world: Vec3,
    pub is_pointing: bool,
}

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
    pub parcel_grass_setting: ParcelGrassSetting,
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
            scene_imposter_distances: vec![100.0, 200.0, 400.0, 800.0, 1600.0, 99999.0],
            scene_imposter_multisample: false,
            scene_imposter_multisample_amount: 0.0,
            scene_imposter_bake: SceneImposterBake::Off,
            parcel_grass_setting: Default::default(),
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
    pub light_count: usize,
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
            light_count: 32,
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

#[derive(Debug)]
pub enum AudioType {
    Voice,
    Scene,
    System,
    Avatar,
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
    #[cfg(not(target_arch = "wasm32"))]
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
    #[serde(default)]
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
pub struct SkyboxConfig {
    pub fixed_time: Option<f32>,
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
    pub skybox_config: Option<SkyboxConfig>,
    pub authoritative_multiplayer: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PermissionValue {
    Allow,
    Deny,
    Ask,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug, EnumIter)]
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

pub trait PermissionStrings {
    fn active(&self) -> &str;
    fn passive(&self) -> &str;
    fn title(&self) -> &str;
    fn request(&self) -> String;
    fn on_success(&self, portable: Option<&str>, additional: Option<&str>) -> String;

    fn on_fail(&self, portable: Option<&str>) -> String;
    fn description(&self) -> String;
}

impl PermissionStrings for PermissionType {
    fn title(&self) -> &str {
        match self {
            PermissionType::MovePlayer => "Move Avatar",
            PermissionType::ForceCamera => "Force Camera",
            PermissionType::PlayEmote => "Play Emote",
            PermissionType::SetLocomotion => "Set Locomotion",
            PermissionType::HideAvatars => "Hide Avatars",
            PermissionType::DisableVoice => "Disable Voice",
            PermissionType::Teleport => "Teleport",
            PermissionType::ChangeRealm => "Change Realm",
            PermissionType::SpawnPortable => "Spawn Portable Experience",
            PermissionType::KillPortables => "Manage Portable Experiences",
            PermissionType::Web3 => "Web3 Transaction",
            PermissionType::Fetch => "Fetch Data",
            PermissionType::Websocket => "Open Websocket",
            PermissionType::OpenUrl => "Open Url",
            PermissionType::CopyToClipboard => "Copy to Clipboard",
        }
    }

    fn request(&self) -> String {
        format!("The scene wants permission to {}", self.passive())
    }

    fn description(&self) -> String {
        format!(
            "This permission is requested when scene attempts to {}",
            self.passive()
        )
    }

    fn on_success(&self, portable: Option<&str>, additional: Option<&str>) -> String {
        format!(
            "{} is {}{}(click to manage)",
            match portable {
                Some(portable) => format!("The portable scene {portable}"),
                None => "The scene".to_owned(),
            },
            self.active(),
            additional
                .map(|add| format!(": {add} "))
                .unwrap_or_default(),
        )
    }
    fn on_fail(&self, portable: Option<&str>) -> String {
        format!(
            "{} was blocked from {} (click to manage)",
            match portable {
                Some(portable) => format!("The portable scene {portable}"),
                None => "The scene".to_owned(),
            },
            self.active()
        )
    }

    fn passive(&self) -> &str {
        match self {
            PermissionType::MovePlayer => "move your avatar within the scene bounds",
            PermissionType::ForceCamera => "temporarily change the camera view",
            PermissionType::PlayEmote => "make your avatar perform an emote",
            PermissionType::SetLocomotion => "temporarily modify your avatar's locomotion settings",
            PermissionType::HideAvatars => "temporarily hide player avatars",
            PermissionType::DisableVoice => "temporarily disable voice chat",
            PermissionType::Teleport => "teleport you to a new location",
            PermissionType::ChangeRealm => "move you to a new realm",
            PermissionType::SpawnPortable => "spawn a portable experience",
            PermissionType::KillPortables => "manage your active portable experiences",
            PermissionType::Web3 => "initiate a web3 transaction with your wallet",
            PermissionType::Fetch => "fetch data from a remote server",
            PermissionType::Websocket => "open a web socket to communicate with a remote server",
            PermissionType::OpenUrl => "open a url in your browser",
            PermissionType::CopyToClipboard => "copy text into the clipboard",
        }
    }

    fn active(&self) -> &str {
        match self {
            PermissionType::MovePlayer => "moving your avatar",
            PermissionType::ForceCamera => "enforcing the camera view",
            PermissionType::PlayEmote => "making your avatar perform an emote",
            PermissionType::SetLocomotion => "enforcing your locomotion settings",
            PermissionType::HideAvatars => "hiding some avatars",
            PermissionType::DisableVoice => "disabling voice communications",
            PermissionType::Teleport => "teleporting you to a new location",
            PermissionType::ChangeRealm => "teleporting you to a new realm",
            PermissionType::SpawnPortable => "spawning a portable experience",
            PermissionType::KillPortables => "managing your active portables",
            PermissionType::Web3 => "initiating a web3 transaction",
            PermissionType::Fetch => "fetching remote data",
            PermissionType::Websocket => "opening a websocket",
            PermissionType::OpenUrl => "opening a url in your browser",
            PermissionType::CopyToClipboard => "copying text into the clipboard",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PermissionLevel {
    Scene(String),
    Realm(String),
    Global,
}

#[derive(Clone, Serialize, Deserialize, Event)]
#[serde(rename_all = "camelCase")]
pub struct PermissionUsed {
    pub ty: PermissionType,
    pub additional: Option<String>,
    pub scene: String,
    pub was_allowed: bool,
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

/// Controls engine-level overrides to avatar movement, independent of scene-driven movement.
#[derive(Resource, Default)]
pub struct EngineMovementControl {
    /// Non-empty means collision/clipping is disabled (e.g. "noclip" from /idnoclip)
    pub suppress_clipping: HashSet<&'static str>,
    /// Non-empty means avatar physics movement systems are suppressed (e.g. "move_player_to" during interpolation)
    pub suppress_avatar_physics: HashSet<&'static str>,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum MoveKind {
    #[default]
    Idle,
    Walk,
    Jog,
    Run,
    Jump,
    /// In-air second jump. Set on foreign avatars when the incoming rfc4::Movement
    /// reports jump_count >= 2 and there's no scene-driven animation to take
    /// precedence; the velocity picker emits the `double_jump` emote for it.
    DoubleJump,
    Falling,
    LongFalling,
    /// Gliding state. Set on foreign avatars when rfc4::Movement.glide_state is
    /// OPENING_PROP or GLIDING and there's no scene-driven animation; the
    /// velocity picker emits the `glide` emote.
    Glide,
    Emote,
}

#[derive(Component, Default)]
pub struct AvatarDynamicState {
    pub velocity: Vec3,
    pub ground_height: f32,
    pub jump_time: f32,
    pub move_kind: MoveKind,
}

#[derive(Event)]
pub enum PreviewCommand {
    ReloadScene { hash: String },
}

pub struct StartupScene {
    pub source: String,
    pub super_user: bool,
    pub preview: bool,
    pub hot_reload: Option<tokio::sync::mpsc::UnboundedSender<PreviewCommand>>,
    pub hash: Option<String>,
}

#[derive(Resource)]
pub struct StartupScenes {
    pub scenes: Vec<StartupScene>,
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
    /// secs since midnight
    pub time: f32,
}

impl TimeOfDay {
    pub fn elapsed_secs(&self) -> f32 {
        self.time
    }
}

/// Fixed time defined on `scene.json` `skyboxConfig`
#[derive(Component)]
#[component(immutable)]
pub struct SceneTime {
    /// secs since midnight
    pub time: f32,
}

// porting aid, used to be one component
pub type TextStyle = (TextFont, TextColor);

// non-spatial audio
#[derive(Component, Debug)]
pub struct AudioEmitter {
    pub handle: Handle<bevy_kira_audio::AudioSource>,
    pub playing: bool,
    pub playback_speed: f32,
    pub r#loop: bool,
    pub volume: f32,
    pub global: bool,
    pub seek_time: Option<f32>,
    pub ty: AudioType,
}

impl Default for AudioEmitter {
    fn default() -> Self {
        Self {
            handle: default(),
            playing: true,
            playback_speed: 1.0,
            r#loop: false,
            volume: 1.0,
            global: false,
            seek_time: None,
            ty: AudioType::System,
        }
    }
}

#[derive(Clone, Copy)]
#[repr(i32)]
pub enum ZOrder {
    Crosshair = -65536,
    // PortableScene -> -65535 <= value <= -1
    // default 0 => appear in world, under scene ui
    OutOfWorldBackdrop = 1,
    SceneUi,
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

#[derive(Event, Debug)]
pub enum AppError {
    NetworkFailure(anyhow::Error),
}

#[derive(Resource, Default, Serialize, Deserialize, Clone)]
pub struct MicState {
    pub available: bool,
    pub enabled: bool,
}

#[derive(Debug, Resource, Default)]
pub struct PreviewMode {
    pub server: Option<String>,
    pub is_preview: bool,
    pub preview_parcel: Option<IVec2>,
}

// resource into which systems can add debug info
#[derive(Resource, Default, Debug)]
pub struct DebugInfo {
    pub info: HashMap<&'static str, String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum GlobalCrdtStateUpdate {
    Crdt(Vec<u8>, dcl_component::Localizer),
    Time(f32),
}

// used for responses to scenes which require strict monotonic timestamps
// by convention we use T = the protobuf struct containing the timestamp (e.g. PbPointerEventsResult, PbTriggerAreaResult)
#[derive(Resource)]
pub struct MonotonicTimestamp<T>(AtomicU32, PhantomData<fn() -> T>);

impl<T> Default for MonotonicTimestamp<T> {
    fn default() -> Self {
        Self(Default::default(), Default::default())
    }
}

impl<T> MonotonicTimestamp<T> {
    pub fn next_timestamp(&self) -> u32 {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParcelGrassSetting {
    Off,
    #[cfg_attr(target_arch = "wasm32", default)]
    Low,
    Mid,
    #[cfg_attr(not(target_arch = "wasm32"), default)]
    High,
}

impl Deref for ParcelGrassSetting {
    type Target = ParcelGrassConfig;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Off => &ParcelGrassConfig {
                layers: 0,
                subdivisions: 32,
                y_displacement: 0.04,
                root_color: Color::Srgba(ParcelGrassConfig::ROOT_COLOR),
                tip_color: Color::Srgba(ParcelGrassConfig::TIP_COLOR),
            },
            Self::Low => &ParcelGrassConfig {
                layers: 8,
                subdivisions: 32,
                y_displacement: 0.02,
                root_color: Color::Srgba(ParcelGrassConfig::ROOT_COLOR),
                tip_color: Color::Srgba(ParcelGrassConfig::TIP_COLOR),
            },
            Self::Mid => &ParcelGrassConfig {
                layers: 16,
                subdivisions: 32,
                y_displacement: 0.02,
                root_color: Color::Srgba(ParcelGrassConfig::ROOT_COLOR),
                tip_color: Color::Srgba(ParcelGrassConfig::TIP_COLOR),
            },
            Self::High => &ParcelGrassConfig {
                layers: 32,
                subdivisions: 32,
                y_displacement: 0.01,
                root_color: Color::Srgba(ParcelGrassConfig::ROOT_COLOR),
                tip_color: Color::Srgba(ParcelGrassConfig::TIP_COLOR),
            },
        }
    }
}

#[derive(Clone, Copy, Resource, Serialize, Deserialize)]
pub struct ParcelGrassConfig {
    pub layers: u32,
    pub subdivisions: u32,
    pub y_displacement: f32,
    pub root_color: Color,
    pub tip_color: Color,
}

impl ParcelGrassConfig {
    pub const ROOT_COLOR: Srgba = palettes::tailwind::LIME_800;
    pub const TIP_COLOR: Srgba = palettes::tailwind::LIME_600;
}

impl Default for ParcelGrassConfig {
    fn default() -> Self {
        Self {
            layers: 32,
            subdivisions: 32,
            y_displacement: 0.01,
            root_color: Self::ROOT_COLOR.into(),
            tip_color: Self::TIP_COLOR.into(),
        }
    }
}

#[derive(Resource, Default, Debug)]
pub struct CurrentRealm {
    pub about_url: String,
    pub address: String,
    pub config: ServerConfiguration,
    pub comms: Option<CommsConfig>,
    pub public_url: String,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CommsConfig {
    pub healthy: bool,
    pub protocol: String,
    pub fixed_adapter: Option<String>,
    pub adapter: Option<String>,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfiguration {
    pub scenes_urn: Option<Vec<String>>,
    pub realm_name: Option<String>,
    pub network_id: Option<u32>,
    pub city_loader_content_server: Option<String>,
    pub map: Option<MapData>,
    pub local_scene_parcels: Option<Vec<String>>,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct MapData {
    pub minimap_enabled: Option<bool>,
    pub sizes: Vec<Region>,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Region {
    pub left: i32,
    pub right: i32,
    pub top: i32,
    pub bottom: i32,
}
