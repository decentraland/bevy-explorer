use std::{
    cell::RefCell,
    rc::Rc,
    sync::{mpsc::SyncSender, Arc},
};

use bevy::log::debug;
use ipfs::{IpfsResource, SceneJsFile};
use system_bridge::SystemApi;
use tokio::sync::{mpsc::Receiver, Mutex};
use wallet::Wallet;

use crate::{
    interface::{crdt_context::CrdtContext, CrdtComponentInterfaces},
    RendererResponse, RpcCalls, SceneElapsedTime, SceneId, SceneLogLevel, SceneLogMessage,
    SceneResponse,
};

use super::interface::CrdtStore;

pub mod engine;
pub mod portables;
pub mod restricted_actions;
pub mod runtime;
pub mod user_identity;

pub mod adaption_layer_helper;
pub mod comms;
pub mod ethereum_controller;
pub mod events;
pub mod player;
pub mod system_api;
pub mod testing;

// marker to indicate shutdown has been triggered
pub struct ShuttingDown;

pub struct RendererStore(pub CrdtStore);

pub struct SuperUserScene(pub tokio::sync::mpsc::UnboundedSender<SystemApi>);
impl std::ops::Deref for SuperUserScene {
    type Target = tokio::sync::mpsc::UnboundedSender<SystemApi>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// marker to notify that the scene/renderer interface functions were used
pub struct CommunicatedWithRenderer;

pub trait State {
    fn borrow<T: 'static>(&self) -> &T;
    fn try_borrow<T: 'static>(&self) -> Option<&T>;
    fn borrow_mut<T: 'static>(&mut self) -> &mut T;
    fn try_borrow_mut<T: 'static>(&mut self) -> Option<&mut T>;
    fn has<T: 'static>(&self) -> bool;
    fn put<T: 'static>(&mut self, value: T);
    fn take<T: 'static>(&mut self) -> T;
    fn try_take<T: 'static>(&mut self) -> Option<T>;
}

#[cfg(feature = "deno")]
use std::ops::{Deref, DerefMut};
#[cfg(feature = "deno")]
impl State for deno_core::OpState {
    fn borrow<T: 'static>(&self) -> &T {
        self.deref().borrow()
    }

    fn try_borrow<T: 'static>(&self) -> Option<&T> {
        self.deref().try_borrow()
    }

    fn borrow_mut<T: 'static>(&mut self) -> &mut T {
        self.deref_mut().borrow_mut()
    }

    fn try_borrow_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.deref_mut().try_borrow_mut()
    }

    fn has<T: 'static>(&self) -> bool {
        self.deref().has::<T>()
    }

    fn put<T: 'static>(&mut self, value: T) {
        self.deref_mut().put(value)
    }

    fn take<T: 'static>(&mut self) -> T {
        self.deref_mut().take()
    }

    fn try_take<T: 'static>(&mut self) -> Option<T> {
        self.deref_mut().try_take()
    }
}

#[allow(clippy::too_many_arguments)]
pub fn init_state(
    state: &mut impl State,
    scene_hash: String,
    scene_id: SceneId,
    storage_root: String,
    scene_js: SceneJsFile,
    crdt_component_interfaces: CrdtComponentInterfaces,
    thread_sx: SyncSender<SceneResponse>,
    thread_rx: Receiver<RendererResponse>,
    global_update_receiver: tokio::sync::broadcast::Receiver<Vec<u8>>,
    ipfs: IpfsResource,
    wallet: Wallet,
    _inspect: bool,
    testing: bool,
    preview: bool,
    super_user: Option<tokio::sync::mpsc::UnboundedSender<SystemApi>>,
) {
    let scene_context = CrdtContext::new(scene_id, scene_hash, testing, preview);
    state.put(scene_context);
    state.put(scene_js);
    state.put(storage_root);
    state.put(crdt_component_interfaces);
    state.put(thread_sx);
    state.put(Arc::new(Mutex::new(thread_rx)));
    state.put(global_update_receiver);
    state.put(ipfs);
    state.put(wallet);
    state.put(CrdtStore::default());
    state.put(RpcCalls::default());
    state.put(RendererStore(CrdtStore::default()));
    state.put(Vec::<SceneLogMessage>::default());
    state.put(SceneElapsedTime(0.0));
    if let Some(super_user) = super_user {
        state.put(SuperUserScene(super_user));
    }
}

pub fn op_log(state: Rc<RefCell<impl State>>, message: String) {
    debug!("op_log {}", message);
    let time = state.borrow().borrow::<SceneElapsedTime>().0;
    state
        .borrow_mut()
        .borrow_mut::<Vec<SceneLogMessage>>()
        .push(SceneLogMessage {
            timestamp: time as f64,
            level: SceneLogLevel::Log,
            message,
        })
}

pub fn op_error(state: Rc<RefCell<impl State>>, message: String) {
    debug!("op_error");
    let time = state.borrow().borrow::<SceneElapsedTime>().0;
    state
        .borrow_mut()
        .borrow_mut::<Vec<SceneLogMessage>>()
        .push(SceneLogMessage {
            timestamp: time as f64,
            level: SceneLogLevel::SceneError,
            message,
        })
}
