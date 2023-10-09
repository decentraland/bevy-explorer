use bevy::prelude::*;
use serde::Serialize;
use std::{
    any::Any,
    marker::PhantomData,
    sync::{Arc, RwLock},
};

use crate::profile::SerializedProfile;

pub trait DynRpcResult: std::any::Any + std::fmt::Debug + Send + Sync + 'static {
    fn as_any(&mut self) -> &mut dyn Any;
}

#[derive(Debug)]
pub struct RpcResult<T: 'static> {
    inner: Option<tokio::sync::oneshot::Sender<T>>,
}

impl<T: std::fmt::Debug + Send + Sync + 'static> DynRpcResult for RpcResult<T> {
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

impl<T: Send + Sync + 'static> RpcResult<T> {
    pub fn new(value: tokio::sync::oneshot::Sender<T>) -> Box<Self> {
        Box::new(Self { inner: Some(value) })
    }
}

// helper to make sending results from systems easy
#[derive(Debug, Clone)]
pub struct RpcResultSender<T> {
    inner: Arc<RwLock<Option<Box<dyn DynRpcResult>>>>,
    _p: PhantomData<fn() -> T>,
}

impl<T: 'static> RpcResultSender<T> {
    pub fn new(sender: Box<dyn DynRpcResult>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Some(sender))),
            _p: Default::default(),
        }
    }

    pub fn send(&self, result: T) {
        if let Ok(mut guard) = self.inner.write() {
            if let Some(mut response) = guard.take() {
                let _ = response
                    .as_any()
                    .downcast_mut::<RpcResult<T>>()
                    .unwrap()
                    .inner
                    .take()
                    .unwrap()
                    .send(result);
            }
        }
    }

    pub fn take(&self) -> tokio::sync::oneshot::Sender<T> {
        self.inner
            .write()
            .ok()
            .and_then(|mut guard| guard.take())
            .and_then(|mut response| {
                response
                    .as_any()
                    .downcast_mut::<RpcResult<T>>()
                    .unwrap()
                    .inner
                    .take()
            })
            .unwrap()
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
pub enum RestrictedAction {
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
}

#[derive(Debug)]
pub enum SceneRpcCall {
    ChangeRealm { to: String, message: Option<String> },
    ExternalUrl { url: String },
    SpawnPortable { location: PortableLocation },
    KillPortable { location: PortableLocation },
    ListPortables,
    GetUserData,
    GetConnectedPlayers,
}
