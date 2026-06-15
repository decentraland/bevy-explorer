use std::{cell::RefCell, rc::Rc, sync::Arc};

use anyhow::anyhow;
use bevy::log::debug;
use common::structs::{CameraFov, GlobalCrdtStateUpdate, TimeOfDay};
use dcl_component::{
    proto_components::sdk::components::PbPlayerIdentityData, DclReader, FromDclReader,
    SceneComponentId, SceneEntityId,
};
use ipfs::SceneJsFile;
use system_bridge::SystemApi;
use tokio::sync::{mpsc::UnboundedReceiver, Mutex};

use crate::{
    interface::{crdt_context::CrdtContext, CrdtComponentInterfaces, CrdtType},
    RendererResponse, RpcCalls, SceneElapsedTime, SceneLogLevel, SceneLogMessage, SceneResponse,
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
pub mod fetch;
pub mod player;
pub mod system_api;
pub mod testing;

#[cfg(target_arch = "wasm32")]
mod response_channel {
    // wasm randomly freezes if we use tokio channels here. no idea why.
    pub type SceneResponseSender = std::sync::mpsc::SyncSender<super::SceneResponse>;
    pub type SceneResponseReceiver = std::sync::mpsc::Receiver<super::SceneResponse>;
    pub type TryRecvError = std::sync::mpsc::TryRecvError;

    pub fn scene_response_channel() -> (super::SceneResponseSender, super::SceneResponseReceiver) {
        std::sync::mpsc::sync_channel(1000)
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod response_channel {
    // we can't use std channels here because the IPC layer wants to select on multiple tokio sources
    pub type SceneResponseSender = tokio::sync::mpsc::Sender<super::SceneResponse>;
    pub type SceneResponseReceiver = tokio::sync::mpsc::Receiver<super::SceneResponse>;
    pub type TryRecvError = tokio::sync::mpsc::error::TryRecvError;

    pub fn scene_response_channel() -> (super::SceneResponseSender, super::SceneResponseReceiver) {
        tokio::sync::mpsc::channel(1000)
    }
}

pub use response_channel::*;

// marker to indicate shutdown has been triggered
pub struct ShuttingDown;

pub struct RendererStore(pub CrdtStore);

// Sidecar holding only the components the scene→renderer filter drops (unrecognized / custom).
// Merged into the inspector snapshot so custom components are visible as raw bytes; never
// pushed to the renderer.
#[derive(Default)]
pub struct FilteredCrdtStore(pub CrdtStore);

// Parallel CrdtContext tracking every scene entity — recognized *and* filtered (custom-only) — so
// the inspector can allocate fresh entity ids via `new_in_range` without colliding with entities
// the main entity_map never sees. Allocation only; its census is discarded.
pub struct AllocatorContext(pub CrdtContext);

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

#[cfg(not(target_arch = "wasm32"))]
use std::ops::{Deref, DerefMut};
#[cfg(not(target_arch = "wasm32"))]
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
    initial_crdt_store: CrdtStore,
    scene_context: CrdtContext,
    storage_root: String,
    scene_js: SceneJsFile,
    crdt_component_interfaces: CrdtComponentInterfaces,
    thread_sx: SceneResponseSender,
    thread_rx: UnboundedReceiver<RendererResponse>,
    global_update_receiver: tokio::sync::broadcast::Receiver<GlobalCrdtStateUpdate>,
    super_user: Option<tokio::sync::mpsc::UnboundedSender<SystemApi>>,
    scene_origin: bevy::prelude::Vec3,
) {
    // Allocator context: a parallel CrdtContext used solely for entity allocation. It's populated
    // with every entity (recognized + filtered) on the send path, but the scene's authored entities
    // load from main.crdt — the scene receives them as initial state and never re-sends them — so
    // seed those here, otherwise new_in_range would hand back ids that already exist.
    let mut allocator = AllocatorContext(CrdtContext::new(
        scene_context.scene_id,
        scene_context.hash.clone(),
        scene_context.title.clone(),
        scene_context.testing,
        scene_context.preview,
    ));
    for lww in initial_crdt_store.lww.values() {
        for entity in lww.last_write.keys() {
            allocator.0.init(*entity);
        }
    }
    for go in initial_crdt_store.go.values() {
        for entity in go.0.keys() {
            allocator.0.init(*entity);
        }
    }
    // flush the seeded entities into the live table so new_in_range avoids them; the census's
    // `born` is exactly the unique set we just seeded.
    let census = allocator.0.take_census();
    debug!(
        "allocator seeded with {} authored entities from main.crdt",
        census.born.len()
    );
    state.put(scene_context);
    state.put(allocator);
    state.put(scene_js);
    state.put(storage_root);
    state.put(crdt_component_interfaces);
    state.put(thread_sx);
    state.put(Arc::new(Mutex::new(thread_rx)));
    state.put(global_update_receiver);
    state.put(CrdtStore::default());
    state.put(RpcCalls::default());
    state.put(RendererStore(initial_crdt_store));
    state.put(FilteredCrdtStore::default());
    state.put(Vec::<SceneLogMessage>::default());
    state.put(SceneElapsedTime(0.0));
    state.put(TimeOfDay { time: 0. });
    state.put(CameraFov::default());
    state.put(dcl_component::SceneOrigin(scene_origin));
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

pub fn player_identity(state: &impl State) -> Result<PbPlayerIdentityData, anyhow::Error> {
    let renderer_store = state.borrow::<RendererStore>();
    let Some(player_identity) = renderer_store.0.get(
        SceneComponentId::PLAYER_IDENTITY_DATA,
        CrdtType::LWW_ANY,
        SceneEntityId::PLAYER,
    ) else {
        anyhow::bail!("no player identity!");
    };
    PbPlayerIdentityData::from_reader(&mut DclReader::new(player_identity))
        .map_err(|e| anyhow!(format!("{e:?}")))
}
