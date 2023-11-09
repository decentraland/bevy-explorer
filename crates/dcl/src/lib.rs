use std::sync::{mpsc::SyncSender, Mutex};

use bevy::{
    prelude::Entity,
    utils::{HashMap, HashSet},
};
use common::rpc::RpcCall;
use deno_core::v8::IsolateHandle;
use once_cell::sync::Lazy;
use tokio::sync::mpsc::Sender;

use dcl_component::SceneEntityId;
use ipfs::{IpfsResource, SceneJsFile};
use wallet::Wallet;

use self::{
    interface::{CrdtComponentInterfaces, CrdtStore},
    js::scene_thread,
};

pub mod crdt;
pub mod interface;
pub mod js;

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy, Debug)]
pub struct SceneId(pub Entity);

impl SceneId {
    pub const DUMMY: SceneId = SceneId(Entity::PLACEHOLDER);
}

// message from scene describing new and deleted entities
pub struct SceneCensus {
    pub scene_id: SceneId,
    pub born: HashSet<SceneEntityId>,
    pub died: HashSet<SceneEntityId>,
}

pub struct SceneElapsedTime(pub f32);

// data from renderer to scene
#[derive(Debug)]
pub enum RendererResponse {
    Ok(CrdtStore),
}

type RpcCalls = Vec<RpcCall>;

#[allow(clippy::large_enum_variant)] // we don't care since the error case is very rare
                                     // data from scene to renderer
pub enum SceneResponse {
    Error(SceneId, String),
    Ok(
        SceneId,
        SceneCensus,
        CrdtStore,
        SceneElapsedTime,
        Vec<SceneLogMessage>,
        RpcCalls,
    ),
    WaitingForInspector,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SceneLogLevel {
    Log,
    SceneError,
    SystemError,
}

#[derive(Clone, Debug)]
pub struct SceneLogMessage {
    pub timestamp: f64, // scene local time
    pub level: SceneLogLevel,
    pub message: String,
}

pub(crate) static VM_HANDLES: Lazy<Mutex<HashMap<SceneId, IsolateHandle>>> =
    Lazy::new(Default::default);

#[allow(clippy::too_many_arguments)]
pub fn spawn_scene(
    scene_hash: String,
    scene_js: SceneJsFile,
    crdt_component_interfaces: CrdtComponentInterfaces,
    renderer_sender: SyncSender<SceneResponse>,
    global_update_receiver: tokio::sync::broadcast::Receiver<Vec<u8>>,
    ipfs: IpfsResource,
    wallet: Wallet,
    id: SceneId,
    inspect: bool,
) -> Sender<RendererResponse> {
    let (main_sx, thread_rx) = tokio::sync::mpsc::channel::<RendererResponse>(1);

    std::thread::Builder::new()
        .name(format!("scene thread {:?}", id.0))
        .spawn(move || {
            scene_thread(
                scene_hash,
                id,
                scene_js,
                crdt_component_interfaces,
                renderer_sender,
                thread_rx,
                global_update_receiver,
                ipfs,
                wallet,
                inspect,
            )
        })
        .unwrap();

    main_sx
}
