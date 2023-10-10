use bevy::prelude::*;
use serde::Serialize;
use std::{
    any::Any,
    sync::{Arc, RwLock},
};

use crate::profile::SerializedProfile;

pub trait DynRpcResult: std::any::Any + std::fmt::Debug + Send + Sync + 'static {
    fn as_any(&mut self) -> &mut dyn Any;
}

#[derive(Debug, Clone)]
pub struct RpcResultSender<T>(Arc<RwLock<Option<tokio::sync::oneshot::Sender<T>>>>);

impl<T: 'static> RpcResultSender<T> {
    pub fn new(sender: tokio::sync::oneshot::Sender<T>) -> Self {
        Self(Arc::new(RwLock::new(Some(sender))))
    }

    pub fn send(&self, result: T) {
        if let Ok(mut guard) = self.0.write() {
            if let Some(response) = guard.take() {
                let _ = response.send(result);
            }
        }
    }

    pub fn take(&self) -> tokio::sync::oneshot::Sender<T> {
        self.0
            .write()
            .ok()
            .and_then(|mut guard| guard.take())
            .take()
            .unwrap()
    }
}

impl<T: 'static> From<tokio::sync::oneshot::Sender<T>> for RpcResultSender<T> {
    fn from(value: tokio::sync::oneshot::Sender<T>) -> Self {
        RpcResultSender::new(value)
    }
}

#[derive(Debug)]
pub enum PortableLocation {
    Urn(String),
    Ens(String),
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SpawnResponse {
    pub pid: String,
    pub parent_cid: String,
    pub name: String,
    pub ens: Option<String>,
}

#[derive(Event, Debug)]
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
        to: Transform,
    },
    MoveCamera(Quat),
    SpawnPortable {
        location: PortableLocation,
        spawner: Option<String>,
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
        response: RpcResultSender<SerializedProfile>,
    },
    GetConnectedPlayers {
        response: RpcResultSender<Vec<String>>,
    },
    SubscribePlayerConnected {
        sender: tokio::sync::mpsc::UnboundedSender<String>,
    },
    SubscribePlayerDisconnected {
        sender: tokio::sync::mpsc::UnboundedSender<String>,
    },
    SubscribePlayerEnteredScene {
        scene: Entity,
        sender: tokio::sync::mpsc::UnboundedSender<String>,
    },
    SubscribePlayerLeftScene {
        scene: Entity,
        sender: tokio::sync::mpsc::UnboundedSender<String>,
    },
}
