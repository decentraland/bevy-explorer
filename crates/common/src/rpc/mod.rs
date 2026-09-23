mod result_sender;
mod stream_sender;
#[cfg(test)]
mod tests;

use crate::{
    profile::SerializedProfile,
    structs::{EmoteMask, PermissionType},
};
use bevy::{platform::collections::HashMap, prelude::*};
use ethers_core::types::H160;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use tokio_util::sync::{CancellationToken, DropGuard};

pub use result_sender::{RpcResultReceiver, RpcResultSender};
pub use stream_sender::{RpcStreamReceiver, RpcStreamSender};

pub trait IpcEndpoint: Send {
    fn send(&mut self, raw_bytes: Vec<u8>);
}

// wraps a registered endpoint so that removing it from the registry (on either side
// closing) cancels `removed`, letting the task that watches for the receiver being
// dropped exit instead of waiting forever
struct RegisteredEndpoint<T: IpcEndpoint> {
    endpoint: T,
    _removed: DropGuard,
}

impl<T: IpcEndpoint> IpcEndpoint for RegisteredEndpoint<T> {
    fn send(&mut self, raw_bytes: Vec<u8>) {
        self.endpoint.send(raw_bytes);
    }
}

pub(crate) fn ipc_register<T: IpcEndpoint + 'static>(
    endpoint: T,
) -> (
    u64,
    tokio::sync::mpsc::UnboundedSender<u64>,
    CancellationToken,
) {
    SCENE_IPC_CONTEXT.with(|cell| {
        let mut ctx = cell.borrow_mut();
        let ctx = ctx.as_mut().unwrap();

        ctx.next_id += 1;
        let id = ctx.next_id;

        let removed = CancellationToken::new();
        ctx.registry.insert(
            id,
            Box::new(RegisteredEndpoint {
                endpoint,
                _removed: removed.clone().drop_guard(),
            }),
        );
        (id, ctx.close_sender.clone(), removed)
    })
}

// notify the engine when the local receiver is dropped early, or stop watching once the
// endpoint has been removed from the registry
pub(crate) fn spawn_close_watcher(
    id: u64,
    cancel: CancellationToken,
    removed: CancellationToken,
    close_sender: tokio::sync::mpsc::UnboundedSender<u64>,
) {
    tokio::spawn(async move {
        tokio::select! {
            _ = cancel.cancelled() => {
                let _ = close_sender.send(id);
            }
            _ = removed.cancelled() => (),
        }
    });
}

pub(crate) fn ipc_router(
    id: u64,
) -> (
    tokio::sync::mpsc::UnboundedSender<(u64, IpcMessage)>,
    CancellationToken,
) {
    ENGINE_IPC_CONTEXT.with(|cell| {
        let mut ctx = cell.borrow_mut();
        let ctx = ctx.as_mut().unwrap();

        // a local sender serialized more than once reuses its id, so every deserialization
        // of that id shares one token and takes a lease on the entry
        let lease = ctx
            .ipc_channel_registry
            .entry(id)
            .and_modify(|lease| lease.count += 1)
            .or_insert_with(|| IpcChannelLease {
                token: CancellationToken::new(),
                count: 1,
            });
        (ctx.ipc_router.clone(), lease.token.clone())
    })
}

// called once all clones of one deserialized engine-side sender for `id` are dropped. only
// the last lease on the id releases the entry and closes the scene-side endpoint; the scene
// host doesn't echo the close back, so drop the entry here rather than waiting for a
// message that never comes. if the scene already closed the channel the entry is gone and
// the scene endpoint with it, so there is nothing left to notify
pub(crate) fn ipc_router_close(
    id: u64,
    router: &tokio::sync::mpsc::UnboundedSender<(u64, IpcMessage)>,
) {
    let released = ENGINE_IPC_CONTEXT
        .try_with(|cell| {
            let mut ctx = cell.borrow_mut();
            let Some(ctx) = ctx.as_mut() else {
                return true;
            };
            let Some(lease) = ctx.ipc_channel_registry.get_mut(&id) else {
                return false;
            };
            lease.count -= 1;
            if lease.count == 0 {
                ctx.ipc_channel_registry.remove(&id);
                true
            } else {
                false
            }
        })
        .unwrap_or(true);
    if released {
        let _ = router.send((id, IpcMessage::Closed));
    }
}

pub struct RequestContext {
    pub registry: HashMap<u64, Box<dyn IpcEndpoint>>,
    pub close_sender: tokio::sync::mpsc::UnboundedSender<u64>,
    pub next_id: u64,
}

// one entry per remote channel id: the token cancelled when the scene-side receiver is
// dropped, shared by every engine-side sender deserialized from that id, and the number of
// those senders still alive
pub struct IpcChannelLease {
    pub token: CancellationToken,
    pub count: usize,
}

pub struct ResponseContext {
    pub ipc_channel_registry: HashMap<u64, IpcChannelLease>,
    pub ipc_router: tokio::sync::mpsc::UnboundedSender<(u64, IpcMessage)>,
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
    pub scene: Entity,
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

#[derive(Event, Debug, Clone, Serialize, Deserialize)]
pub enum RpcCall {
    ChangeRealm {
        scene: Entity,
        to: String,
        message: Option<String>,
        response: RpcResultSender<Result<(), String>>,
    },
    ExternalUrl {
        scene: Entity,
        url: String,
        response: RpcResultSender<Result<(), String>>,
    },
    MovePlayer {
        scene: Option<Entity>,
        to: Vec3,
        looking_at: Option<Vec3>,
        duration: Option<f32>,
        camera_rotation: Option<Quat>,
        response: Option<RpcResultSender<bool>>,
    },
    WalkPlayer {
        scene: Option<Entity>,
        to: Vec3,
        stop_threshold: f32,
        timeout: Option<f32>,
        response: RpcResultSender<bool>,
    },
    TeleportPlayer {
        scene: Option<Entity>,
        /// The parcel to land on; `None` (with a realm) is the realm's default spawn.
        to: Option<IVec2>,
        /// The realm `to` belongs to: a realm change (a full reconnect, as for `ChangeRealm`) happens first.
        realm: Option<String>,
        response: RpcResultSender<Result<(), String>>,
    },
    MoveCamera {
        scene: Entity,
        facing: Quat,
    },
    SpawnPortable {
        location: PortableLocation,
        spawner: Entity,
        response: RpcResultSender<Result<SpawnResponse, String>>,
    },
    KillPortable {
        scene: Entity,
        location: PortableLocation,
        response: RpcResultSender<bool>,
    },
    ListPortables {
        response: RpcResultSender<Vec<SpawnResponse>>,
    },
    GetUserData {
        user: Option<String>,
        scene: Entity,
        response: RpcResultSender<Result<SerializedProfile, ()>>,
    },
    GetConnectedPlayers {
        /// calling scene — scopes the answer to its own room in partitioned mode
        scene: Entity,
        response: RpcResultSender<Vec<String>>,
    },
    GetPlayersInScene {
        scene: Entity,
        response: RpcResultSender<Vec<String>>,
    },
    OpenNftDialog {
        scene: Entity,
        urn: String,
        response: RpcResultSender<Result<(), String>>,
    },
    OpenExplorerUi {
        scene: Entity,
        /// raw `decentraland.sdk.components.common.ExplorerUi` value
        ui: i32,
        response: RpcResultSender<OpenExplorerUiResult>,
    },
    SubscribePlayerConnected {
        scene: Entity,
        sender: RpcEventSender,
    },
    SubscribePlayerDisconnected {
        scene: Entity,
        sender: RpcEventSender,
    },
    SubscribePlayerEnteredScene {
        scene: Entity,
        sender: RpcEventSender,
    },
    SubscribePlayerLeftScene {
        scene: Entity,
        sender: RpcEventSender,
    },
    SubscribeSceneReady {
        scene: Entity,
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
        scene: Entity,
        data: Vec<u8>,
        recipient: Option<H160>,
    },
    SubscribeMessageBus {
        hash: String,
        sender: RpcEventSender,
    },
    SubscribeBinaryBus {
        hash: String,
        sender: RpcStreamSender<(String, Vec<u8>)>,
    },
    TestPlan {
        scene: Entity,
        plan: Vec<String>,
    },
    TestResult {
        scene: Entity,
        name: String,
        success: bool,
        error: Option<String>,
    },
    TestSnapshot(CompareSnapshot),
    SendAsync {
        body: RPCSendableMessage,
        scene: Entity,
        response: RpcResultSender<Result<serde_json::Value, String>>,
    },
    GetTextureSize {
        scene: Entity,
        src: String,
        response: RpcResultSender<Result<Vec2, String>>,
    },
    RequestGenericPermission {
        scene: Entity,
        ty: PermissionType,
        message: Option<String>,
        response: RpcResultSender<bool>,
    },
    TriggerEmote {
        scene: Entity,
        urn: String,
        r#loop: bool,
        mask: EmoteMask,
    },
    StopEmote {
        scene: Entity,
    },
    UiFocus {
        scene: Entity,
        action: RpcUiFocusAction,
        response: RpcResultSender<Result<Option<String>, String>>,
    },
    CopyToClipboard {
        scene: Entity,
        text: String,
        response: RpcResultSender<Result<(), String>>,
    },
    SignRequest {
        method: String,
        uri: String,
        meta: Option<String>,
        /// scene hash of the requesting scene, when the request originates from scene JS —
        /// selects a per-scene storage delegation in server mode
        scene: Option<String>,
        response: RpcResultSender<Result<Vec<(String, String)>, String>>,
    },
    ReadFile {
        scene_hash: String,
        filename: String,
        response: RpcResultSender<Result<ReadFileResponse, String>>,
    },
    EntityDefinition {
        urn: String,
        response: RpcResultSender<Option<EntityDefinitionResponse>>,
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
