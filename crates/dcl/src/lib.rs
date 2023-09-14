use std::sync::{
    atomic::{AtomicU32, Ordering},
    mpsc::SyncSender,
    Mutex,
};

use bevy::{
    prelude::AssetServer,
    utils::{HashMap, HashSet},
};
use deno_core::v8::IsolateHandle;
use once_cell::sync::Lazy;
use tokio::sync::mpsc::Sender;

use dcl_component::SceneEntityId;
use ipfs::SceneJsFile;

use self::{
    interface::{CrdtComponentInterfaces, CrdtStore},
    js::{create_runtime, scene_thread},
};

pub mod crdt;
pub mod interface;
pub mod js;

#[derive(Default, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy, Debug)]
pub struct SceneId(u32);

impl SceneId {
    pub const DUMMY: SceneId = SceneId(0);
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
    ),
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

static SCENE_ID: Lazy<AtomicU32> = Lazy::new(Default::default);
pub(crate) static VM_HANDLES: Lazy<Mutex<HashMap<SceneId, IsolateHandle>>> =
    Lazy::new(Default::default);

pub fn get_next_scene_id() -> SceneId {
    let mut id = SceneId(SCENE_ID.fetch_add(1, Ordering::Relaxed));

    if id.0 == 0 {
        // synchronously create and drop a single runtime to hopefully avoid initial segfaults
        create_runtime(true);
        // and skip the dummy id
        id = SceneId(SCENE_ID.fetch_add(1, Ordering::Relaxed));
    }

    id
}

pub fn spawn_scene(
    scene_hash: String,
    scene_js: SceneJsFile,
    crdt_component_interfaces: CrdtComponentInterfaces,
    renderer_sender: SyncSender<SceneResponse>,
    global_update_receiver: tokio::sync::broadcast::Receiver<Vec<u8>>,
    asset_server: AssetServer,
    id: SceneId,
) -> Sender<RendererResponse> {
    let (main_sx, thread_rx) = tokio::sync::mpsc::channel::<RendererResponse>(1);

    std::thread::Builder::new()
        .name(format!("scene thread {}", id.0))
        .spawn(move || {
            scene_thread(
                scene_hash,
                id,
                scene_js,
                crdt_component_interfaces,
                renderer_sender,
                thread_rx,
                global_update_receiver,
                asset_server,
            )
        })
        .unwrap();

    main_sx
}
