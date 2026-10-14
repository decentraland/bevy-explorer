mod result_sender;
mod stream_sender;
#[cfg(test)]
mod tests;

use crate::{
    profile::SerializedProfile,
    structs::{EmoteMask, PermissionType},
};
use alloy_core::primitives::Address;
use bevy::{platform::collections::HashMap, prelude::*};
use dcl_component::proto_components::kernel::apis::VideoTracksActiveStreamsResponse;
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, sync::Arc};
pub use tokio_util::sync::CancellationToken;

pub use result_sender::{RpcResultReceiver, RpcResultSender};
pub use stream_sender::{RpcStreamReceiver, RpcStreamSender};

pub trait IpcEndpoint: Send {
    fn send(&mut self, raw_bytes: Vec<u8>);
}

pub(crate) fn ipc_register<T: IpcEndpoint + 'static>(
    endpoint: T,
) -> (u64, tokio::sync::mpsc::UnboundedSender<u64>) {
    SCENE_IPC_CONTEXT.with(|cell| {
        let mut ctx = cell.borrow_mut();
        let ctx = ctx.as_mut().unwrap();

        ctx.next_id += 1;
        let id = ctx.next_id;

        ctx.registry.insert(id, Box::new(endpoint));
        (id, ctx.close_sender.clone())
    })
}

type IpcRouter = tokio::sync::mpsc::UnboundedSender<(u64, IpcMessage)>;

pub(crate) fn ipc_router(id: u64) -> Result<(IpcRouter, CancellationToken), &'static str> {
    ENGINE_IPC_CONTEXT
        .try_with(|cell| {
            let mut ctx = cell.try_borrow_mut().map_err(|_| "ipc context in use")?;
            let ctx = ctx.as_mut().ok_or("no ipc context")?;

            if let Some(max) = ctx.max_channels {
                if ctx.ipc_channel_registry.contains_key(&id) {
                    return Err("ipc channel id reused");
                }
                if ctx.ipc_channel_registry.len() >= max {
                    return Err("too many open ipc channels");
                }
            }
            let token = ctx
                .parent_token
                .as_ref()
                .map_or_else(CancellationToken::new, CancellationToken::child_token);
            if ctx.ipc_channel_registry.insert(id, token.clone()).is_some() {
                warn!("ipc channel {id} deserialized twice; the first remote's close will cut off the second");
            }
            Ok((ctx.ipc_router.clone(), token))
        })
        .map_err(|_| "no ipc context")?
}

pub struct RequestContext {
    pub registry: HashMap<u64, Box<dyn IpcEndpoint>>,
    pub close_sender: tokio::sync::mpsc::UnboundedSender<u64>,
    pub next_id: u64,
}

pub struct ResponseContext {
    pub ipc_channel_registry: HashMap<u64, CancellationToken>,
    pub ipc_router: tokio::sync::mpsc::UnboundedSender<(u64, IpcMessage)>,
    /// For an untrusted remote: decoding a channel fails once this many are open, or if its id
    /// is already open.
    pub max_channels: Option<usize>,
    /// Channels decoded here close when this is cancelled, as well as on the remote's close.
    pub parent_token: Option<CancellationToken>,
}

#[derive(Serialize, Deserialize)]
pub enum IpcMessage {
    Data(Vec<u8>),
    Closed,
}

thread_local! {
    // Context for Serialization
    pub static SCENE_IPC_CONTEXT: RefCell<Option<RequestContext>> = const { RefCell::new(None) };
    // Context for Deserialization
    pub static ENGINE_IPC_CONTEXT: RefCell<Option<ResponseContext>> = const { RefCell::new(None) };
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PortableLocation {
    Urn(String),
    Ens(String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SpawnResponse {
    pub pid: String,
    pub parent_cid: String,
    pub name: String,
    pub ens: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompareSnapshot {
    pub camera_position: [f32; 3],
    pub camera_target: [f32; 3],
    pub snapshot_size: [u32; 2],
    pub name: String,
    pub response: RpcResultSender<CompareSnapshotResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompareSnapshotResult {
    pub error: Option<String>,
    pub found: bool,
    pub similarity: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RPCSendableMessage {
    pub method: String,
    pub params: Vec<serde_json::Value>, // Using serde_json::Value for unknown[]
}

impl RPCSendableMessage {
    /// The methods a scene may ask the user's wallet to sign (`RpcCall::SendAsync`).
    pub const SIGNING_METHODS: &[&str] = &["eth_sendTransaction", "eth_signTypedData_v4"];
}

pub type RpcEventSender = RpcStreamSender<String>;

/// `decentraland.kernel.apis.OpenExplorerUiResult` (restricted_actions.proto; the kernel api
/// protos are not compiled, so the wire values are mirrored here).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum OpenExplorerUiResult {
    Unspecified = 0,
    Opened = 1,
    WasAlreadyOpen = 2,
    RejectedNotCurrentScene = 3,
    RejectedFeatureDisabled = 4,
    RejectedNoUserGesture = 5,
}

/// Who issued an [`RpcCall`]. Attached by the engine where it receives the call, never
/// supplied by the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RpcOrigin {
    /// the calling scene: its root entity and its scene hash
    Scene { entity: Entity, hash: Arc<str> },
    /// the engine itself: console and agent commands, system ui, automatic testing
    Engine,
}

impl RpcOrigin {
    pub fn scene(&self) -> Option<Entity> {
        match self {
            Self::Scene { entity, .. } => Some(*entity),
            Self::Engine => None,
        }
    }
}

#[derive(Event, Debug, Clone)]
pub struct RpcCallEvent {
    pub origin: RpcOrigin,
    pub call: RpcCall,
}

impl RpcCallEvent {
    pub fn engine(call: RpcCall) -> Self {
        Self {
            origin: RpcOrigin::Engine,
            call,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RpcCall {
    ChangeRealm {
        to: String,
        message: Option<String>,
        response: RpcResultSender<Result<(), String>>,
    },
    ExternalUrl {
        url: String,
        response: RpcResultSender<Result<(), String>>,
    },
    MovePlayer {
        to: Vec3,
        looking_at: Option<Vec3>,
        duration: Option<f32>,
        camera_rotation: Option<Quat>,
        response: Option<RpcResultSender<bool>>,
    },
    WalkPlayer {
        to: Vec3,
        stop_threshold: f32,
        timeout: Option<f32>,
        response: RpcResultSender<bool>,
    },
    TeleportPlayer {
        /// The parcel to land on; `None` (with a realm) is the realm's default spawn.
        to: Option<IVec2>,
        /// The realm `to` belongs to: a realm change (a full reconnect, as for `ChangeRealm`) happens first.
        realm: Option<String>,
        response: RpcResultSender<Result<(), String>>,
    },
    MoveCamera {
        facing: Quat,
    },
    SpawnPortable {
        location: PortableLocation,
        response: RpcResultSender<Result<SpawnResponse, String>>,
    },
    KillPortable {
        location: PortableLocation,
        response: RpcResultSender<bool>,
    },
    ListPortables {
        response: RpcResultSender<Vec<SpawnResponse>>,
    },
    GetUserData {
        user: Option<String>,
        response: RpcResultSender<Result<SerializedProfile, ()>>,
    },
    GetConnectedPlayers {
        response: RpcResultSender<Vec<String>>,
    },
    GetPlayersInScene {
        response: RpcResultSender<Vec<String>>,
    },
    OpenNftDialog {
        urn: String,
        response: RpcResultSender<Result<(), String>>,
    },
    OpenExplorerUi {
        /// raw `decentraland.sdk.components.common.ExplorerUi` value
        ui: i32,
        response: RpcResultSender<OpenExplorerUiResult>,
    },
    SubscribePlayerConnected {
        sender: RpcEventSender,
    },
    SubscribePlayerDisconnected {
        sender: RpcEventSender,
    },
    SubscribePlayerEnteredScene {
        sender: RpcEventSender,
    },
    SubscribePlayerLeftScene {
        sender: RpcEventSender,
    },
    SubscribeSceneReady {
        sender: RpcEventSender,
    },
    SubscribePlayerExpression {
        sender: RpcEventSender,
    },
    SubscribeProfileChanged {
        sender: RpcEventSender,
    },
    SubscribeRealmChanged {
        sender: RpcEventSender,
    },
    SubscribePlayerClicked {
        sender: RpcEventSender,
    },
    SendMessageBus {
        data: Vec<u8>,
        recipient: Option<Address>,
    },
    SubscribeMessageBus {
        sender: RpcEventSender,
    },
    SubscribeBinaryBus {
        sender: RpcStreamSender<(String, Vec<u8>)>,
    },
    TestPlan {
        plan: Vec<String>,
    },
    TestResult {
        name: String,
        success: bool,
        error: Option<String>,
    },
    TestSnapshot(CompareSnapshot),
    SendAsync {
        body: RPCSendableMessage,
        response: RpcResultSender<Result<serde_json::Value, String>>,
    },
    GetTextureSize {
        src: String,
        response: RpcResultSender<Result<Vec2, String>>,
    },
    RequestGenericPermission {
        ty: PermissionType,
        message: Option<String>,
        response: RpcResultSender<bool>,
    },
    TriggerEmote {
        urn: String,
        r#loop: bool,
        mask: EmoteMask,
    },
    StopEmote,
    UiFocus {
        action: RpcUiFocusAction,
        response: RpcResultSender<Result<Option<String>, String>>,
    },
    CopyToClipboard {
        text: String,
        response: RpcResultSender<Result<(), String>>,
    },
    SignRequest {
        method: String,
        uri: String,
        response: RpcResultSender<Result<Vec<(String, String)>, String>>,
    },
    ReadFile {
        filename: String,
        response: RpcResultSender<Result<ReadFileResponse, String>>,
    },
    EntityDefinition {
        urn: String,
        response: RpcResultSender<Option<EntityDefinitionResponse>>,
    },
    ActiveVideoStreams {
        response: RpcResultSender<VideoTracksActiveStreamsResponse>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RpcUiFocusAction {
    Focus { element_id: String },
    Defocus,
    GetFocus,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ReadFileResponse {
    pub content: Vec<u8>,
    pub hash: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct EntityDefinitionResponse {
    pub collection: HashMap<String, String>,
    pub metadata: Option<String>,
    pub base_url: String,
}

// rmp can't handle #[serde(flatten)] and json::Value without using named vec.
// the message format is much larger but flattens and values are rare in our ipc api,
// so we default try to encode without
pub fn rmp_encode<T: Serialize>(value: &T) -> Result<Vec<u8>, rmp_serde::encode::Error> {
    rmp_serde::to_vec(value).or_else(|_| rmp_serde::to_vec_named(value))
}
