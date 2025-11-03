use bevy::prelude::*;
use ethers_core::types::H160;
use platform::AsyncRwLock;
use serde::{Deserialize, Serialize};
use std::{any::Any, sync::Arc};

use crate::{profile::SerializedProfile, structs::PermissionType};

pub trait DynRpcResult: std::any::Any + std::fmt::Debug + Send + Sync + 'static {
    fn as_any(&mut self) -> &mut dyn Any;
}

#[derive(Clone)]
pub struct RpcResultSender<T>(Arc<AsyncRwLock<Option<tokio::sync::oneshot::Sender<T>>>>);

impl<T> std::fmt::Debug for RpcResultSender<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("RpcResultSender").finish()
    }
}

impl<T: 'static> Default for RpcResultSender<T> {
    fn default() -> Self {
        Self(Arc::new(AsyncRwLock::new(None)))
    }
}

impl<T: 'static> RpcResultSender<T> {
    pub fn new(sender: tokio::sync::oneshot::Sender<T>) -> Self {
        Self(Arc::new(AsyncRwLock::new(Some(sender))))
    }

    pub fn send(&self, result: T) {
        let mut guard = self.0.blocking_write();
        if let Some(response) = guard.take() {
            let _ = response.send(result);
        }
    }

    pub fn take(&self) -> tokio::sync::oneshot::Sender<T> {
        self.0.blocking_write().take().unwrap()
    }
}

impl<T: 'static> From<tokio::sync::oneshot::Sender<T>> for RpcResultSender<T> {
    fn from(value: tokio::sync::oneshot::Sender<T>) -> Self {
        RpcResultSender::new(value)
    }
}

#[derive(Debug, Clone)]
pub enum PortableLocation {
    Urn(String),
    Ens(String),
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SpawnResponse {
    pub pid: String,
    pub parent_cid: String,
    pub name: String,
    pub ens: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CompareSnapshot {
    pub scene: Entity,
    pub camera_position: [f32; 3],
    pub camera_target: [f32; 3],
    pub snapshot_size: [u32; 2],
    pub name: String,
    pub response: RpcResultSender<CompareSnapshotResult>,
}

#[derive(Debug, Clone)]
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

pub type RpcEventSender = tokio::sync::mpsc::UnboundedSender<String>;

#[derive(Event, Debug, Clone)]
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
        sender: tokio::sync::mpsc::UnboundedSender<(String, Vec<u8>)>,
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
    SetUiFocus {
        scene: Entity,
        element_id: Option<String>,
        response: RpcResultSender<Result<(), String>>,
    },
    CopyToClipboard {
        scene: Entity,
        text: String,
        response: RpcResultSender<Result<(), String>>,
    },
}
