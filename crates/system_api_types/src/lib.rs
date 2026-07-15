//! Plain serde data structs that cross the `~system/BevyExplorerApi` boundary.
//!
//! Split out of `system_bridge` so they can be compiled without bevy and fed to
//! `ts-rs` (scripts/gen-ts-bindings.sh) to generate the TypeScript the react-web
//! page and bridge scene consume. `system_bridge` re-exports everything here.

use dcl_component::proto_components::{
    common::{Color3, Vector2, Vector3},
    sdk::components::{pb_pointer_events, PbAvatarBase, PbAvatarEquippedData},
};
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

/// A Color3 with a `clear` flag, distinguishing "set this color" from "clear the
/// stored color" while remaining representable in JSON (where an absent field
/// already means "leave unchanged").
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ts_rs::TS)]
pub struct ClearableColor3 {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub clear: bool,
}

impl ClearableColor3 {
    pub fn to_color3(self) -> Option<Color3> {
        if self.clear {
            None
        } else {
            Some(Color3 {
                r: self.r,
                g: self.g,
                b: self.b,
            })
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct SetAvatarData {
    pub base: Option<PbAvatarBase>,
    pub equip: Option<PbAvatarEquippedData>,
    pub has_claimed_name: Option<bool>,
    pub profile_extras: Option<std::collections::HashMap<String, serde_json::Value>>,
    pub name_color: Option<ClearableColor3>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[derive(ts_rs::TS)]
#[ts(export)]
pub struct LiveSceneInfo {
    pub hash: String,
    pub base_url: Option<String>,
    pub title: String,
    pub parcels: Vec<Vector2>,
    pub is_portable: bool,
    pub is_broken: bool,
    pub is_blocked: bool,
    pub is_super: bool,
    pub sdk_version: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct HomeScene {
    pub realm: String,
    pub parcel: Vector2,
}

#[derive(Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct ChatMessage {
    pub sender_address: String,
    pub message: String,
    pub channel: String,
}

#[derive(Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct VoiceMessage {
    pub sender_address: String,
    pub channel: String,
    pub active: bool,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
#[derive(ts_rs::TS)]
#[ts(export)]
pub struct HoverAction {
    #[serde(flatten)]
    pub event: pb_pointer_events::Entry,
    pub enabled: bool,
}

/// Classifier for pointer-target hits — distinguishes scene world geometry,
/// scene UI overlays, and avatars. The engine's pointer pipeline produces it
/// before any scene API surface is involved; `common::structs` re-exports it
/// for engine use, and `HoverEvent` carries the same value across the
/// system-api boundary.
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

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
#[derive(ts_rs::TS)]
#[ts(export)]
pub struct HoverEvent {
    pub entered: bool,
    #[ts(type = "number")]
    pub target_type: PointerTargetType,
    pub actions: Vec<HoverAction>,
}

/// Streamed to the system scene whenever an entity carrying PROXIMITY pointer
/// entries enters or leaves the avatar's interaction range, or when one of its
/// per-entry distance gates flips (so the `enabled` flag on an action changes).
/// `entity` is an opaque session-stable identifier so the scene can match
/// enter/leave pairs. `entity_position` is the world-space AABB centre of the
/// specific collider on the entity that produced the closest-point hit — for
/// multi-collider entities (e.g. GltfContainers) this anchors UI on the part
/// of the entity the player is actually nearest. The scene is responsible for
/// projecting it to screen space per frame; the active camera's vertical FOV
/// is available via `Runtime.getCameraFov`.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
#[derive(ts_rs::TS)]
#[ts(export)]
pub struct ProximityEvent {
    pub entered: bool,
    pub entity: u32,
    pub entity_position: Vector3,
    pub actions: Vec<HoverAction>,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
#[derive(ts_rs::TS)]
#[ts(export)]
pub struct SceneLoadingUi {
    pub visible: bool,
    pub realm_connected: bool,
    pub title: String,
    pub pending_assets: Option<u32>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[derive(ts_rs::TS)]
#[ts(export)]
pub struct AvatarModifierState {
    pub user_id: String,
    pub hide_avatar: bool,
    pub hide_profile: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(ts_rs::TS)]
#[ts(export)]
pub struct NameColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(ts_rs::TS)]
#[ts(export)]
pub struct FriendData {
    pub address: String,
    pub name: String,
    pub has_claimed_name: bool,
    pub profile_picture_url: String,
    pub name_color: Option<NameColor>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(ts_rs::TS)]
#[ts(export)]
pub struct FriendRequestData {
    pub address: String,
    pub name: String,
    pub has_claimed_name: bool,
    pub profile_picture_url: String,
    pub name_color: Option<NameColor>,
    #[ts(type = "number")]
    pub created_at: i64,
    pub message: Option<String>,
    pub id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all_fields = "camelCase")]
#[serde(tag = "type")]
#[derive(ts_rs::TS)]
#[ts(export)]
pub enum FriendshipEventUpdate {
    #[serde(rename = "request")]
    Request {
        address: String,
        name: String,
        has_claimed_name: bool,
        profile_picture_url: String,
        name_color: Option<NameColor>,
        #[ts(type = "number")]
        created_at: i64,
        message: Option<String>,
        id: String,
    },
    #[serde(rename = "accept")]
    Accept { address: String },
    #[serde(rename = "reject")]
    Reject { address: String },
    #[serde(rename = "cancel")]
    Cancel { address: String },
    #[serde(rename = "delete")]
    Delete { address: String },
    #[serde(rename = "block")]
    Block { address: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(ts_rs::TS)]
#[ts(export)]
pub struct FriendStatusData {
    pub address: String,
    pub name: String,
    pub has_claimed_name: bool,
    pub profile_picture_url: String,
    pub name_color: Option<NameColor>,
    /// "online", "offline", or "away"
    pub status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(ts_rs::TS)]
#[ts(export)]
pub struct BlockedUserData {
    pub address: String,
    pub name: String,
    pub has_claimed_name: bool,
    pub profile_picture_url: String,
    pub name_color: Option<NameColor>,
}

/// Both directions of the blocking relationship for the local user,
/// addresses only (no profiles).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(ts_rs::TS)]
#[ts(export)]
pub struct BlockingStatusData {
    pub blocked_users: Vec<String>,
    pub blocked_by_users: Vec<String>,
}

/// Emitted when another user blocks / unblocks the local user.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(ts_rs::TS)]
#[ts(export)]
pub struct BlockUpdateData {
    pub address: String,
    pub is_blocked: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(ts_rs::TS)]
#[ts(export)]
pub struct FriendConnectivityEvent {
    pub address: String,
    pub name: String,
    pub has_claimed_name: bool,
    pub profile_picture_url: String,
    pub name_color: Option<NameColor>,
    /// "online", "offline", or "away"
    pub status: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug, ts_rs::TS)]
pub enum PermissionValue {
    Allow,
    Deny,
    Ask,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug, EnumIter, ts_rs::TS)]
pub enum PermissionType {
    MovePlayer,
    ForceCamera,
    PlayEmote,
    SetLocomotion,
    HideAvatarsNametags,
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

#[derive(Clone, Debug, Serialize, Deserialize, ts_rs::TS)]
pub enum PermissionLevel {
    Scene(String),
    Realm(String),
    Global,
}

#[derive(Serialize, Deserialize, Clone, ts_rs::TS)]
#[ts(export)]
pub struct PermanentPermissionItem {
    pub ty: PermissionType,
    pub allow: PermissionValue,
}

#[derive(Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct PermissionRequest {
    pub ty: PermissionType,
    pub additional: Option<String>,
    pub scene: String,
    pub id: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct SetSinglePermission {
    pub id: usize,
    pub allow: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct SetPermanentPermission {
    pub ty: PermissionType,
    pub level: PermissionLevel,
    pub allow: Option<PermissionValue>,
}

#[derive(Serialize, Deserialize, Clone, ts_rs::TS)]
#[ts(export)]
pub struct NamedVariant {
    pub name: String,
    pub description: String,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[derive(ts_rs::TS)]
#[ts(export)]
pub struct SettingInfo {
    pub name: String,
    pub category: String,
    pub description: String,
    pub min_value: f32,
    pub max_value: f32,
    pub named_variants: Vec<NamedVariant>,
    pub step_size: f32,
    pub value: f32,
    pub default: f32,
}
