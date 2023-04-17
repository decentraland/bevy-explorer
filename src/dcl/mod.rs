use std::sync::{
    atomic::{AtomicU32, Ordering},
    mpsc::SyncSender,
    Mutex,
};

use bevy::utils::{HashMap, HashSet};
use deno_core::v8::IsolateHandle;
use once_cell::sync::Lazy;
use tokio::sync::mpsc::Sender;

use crate::{dcl_component::SceneEntityId, ipfs::SceneJsFile};

use self::{
    interface::{CrdtComponentInterfaces, CrdtStore},
    js::{create_runtime, scene_thread},
};

pub mod crdt;
pub mod interface;
pub mod js;

#[derive(Default, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy, Debug)]
pub struct SceneId(u32);

// message from scene describing new and deleted entities
pub struct SceneCensus {
    pub scene_id: SceneId,
    pub born: HashSet<SceneEntityId>,
    pub died: HashSet<SceneEntityId>,
}

// data from renderer to scene
#[derive(Debug)]
pub enum RendererResponse {
    Ok(CrdtStore),
}

// data from scene to renderer
pub enum SceneResponse {
    Error(SceneId, String),
    Ok(SceneId, SceneCensus, CrdtStore),
}

static SCENE_ID: Lazy<AtomicU32> = Lazy::new(Default::default);
pub(crate) static VM_HANDLES: Lazy<Mutex<HashMap<SceneId, IsolateHandle>>> =
    Lazy::new(Default::default);

pub fn spawn_scene(
    scene_js: SceneJsFile,
    crdt_component_interfaces: CrdtComponentInterfaces,
    renderer_sender: SyncSender<SceneResponse>,
) -> (SceneId, Sender<RendererResponse>) {
    let id = SceneId(SCENE_ID.fetch_add(1, Ordering::Relaxed));

    if id.0 == 0 {
        // synchronously create and drop a single runtime to hopefully avoid initial segfaults
        create_runtime();
    }

    let (main_sx, thread_rx) = tokio::sync::mpsc::channel::<RendererResponse>(1);

    std::thread::Builder::new()
        .name(format!("scene thread {}", id.0))
        .spawn(move || {
            scene_thread(
                id,
                scene_js,
                crdt_component_interfaces,
                renderer_sender,
                thread_rx,
            )
        })
        .unwrap();

    (id, main_sx)
}
