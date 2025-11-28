mod result_sender;
mod stream_sender;

use bevy::{platform::collections::HashMap, prelude::*};
use ethers_core::types::H160;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use std::cell::RefCell;
use crate::{profile::SerializedProfile, structs::PermissionType};

pub use result_sender::{RpcResultSender, RpcResultReceiver};
pub use stream_sender::{RpcStreamSender, RpcStreamReceiver};

pub trait IpcEndpoint: Send {
    fn send(&mut self, raw_bytes: Vec<u8>);
}

pub(crate) fn ipc_register<T: IpcEndpoint + 'static>(endpoint: T) -> (u64, tokio::sync::mpsc::UnboundedSender<u64>) {
    SCENE_CONTEXT.with(|cell| {
        let mut ctx = cell.borrow_mut();
        let ctx = ctx.as_mut().unwrap();

        ctx.next_id += 1;
        let id = ctx.next_id;

        ctx.registry.insert(id, Box::new(endpoint));
        (id, ctx.close_sender.clone())
    })
}

pub(crate) fn ipc_router(id: u64) -> (tokio::sync::mpsc::UnboundedSender<(u64, IpcMessage)>, CancellationToken) {
    ENGINE_CONTEXT.with(|cell| {
        let mut ctx = cell.borrow_mut();
        let ctx = ctx.as_mut().unwrap();

        let token = CancellationToken::new();
        ctx.registry.insert(id, token.clone());
        (ctx.router.clone(), token)
    })
}




struct RequestContext {
    registry: HashMap<u64, Box<dyn IpcEndpoint>>,
    close_sender: tokio::sync::mpsc::UnboundedSender<u64>,
    next_id: u64,
}

struct ResponseContext {
    registry: HashMap<u64, CancellationToken>,
    router: tokio::sync::mpsc::UnboundedSender<(u64, IpcMessage)>,
}

pub enum IpcMessage {
    Data(Vec<u8>),
    Closed,
}

thread_local! {
    // Context for Serialization
    static SCENE_CONTEXT: RefCell<Option<RequestContext>> = RefCell::new(None);
    // Context for Deserialization
    static ENGINE_CONTEXT: RefCell<Option<ResponseContext>> = RefCell::new(None);
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
        scene: Entity,
        to: Vec3,
        looking_at: Option<Vec3>,
    },
    TeleportPlayer {
        scene: Option<Entity>,
        to: IVec2,
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
    SubscribePlayerConnected {
        sender: RpcEventSender,
    },
    SubscribePlayerDisconnected {
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
    }
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