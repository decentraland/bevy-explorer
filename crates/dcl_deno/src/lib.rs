pub mod js;

use std::{
    panic::{self, AssertUnwindSafe},
    sync::Mutex,
};

use bevy::{log::error, platform::collections::HashMap};
use common::structs::GlobalCrdtStateUpdate;
use deno_core::v8::IsolateHandle;
use once_cell::sync::Lazy;
use system_bridge::SystemApi;
use tokio::sync::mpsc::Sender;

use ipfs::SceneJsFile;

use dcl::{
    interface::{CrdtComponentInterfaces, CrdtStore},
    js::SceneResponseSender,
    RendererResponse, SceneId,
};

use crate::js::scene_thread;

pub(crate) static VM_HANDLES: Lazy<Mutex<HashMap<SceneId, IsolateHandle>>> =
    Lazy::new(Default::default);

/// must be called from main thread on linux before any isolates are created
pub fn init_runtime() {
    let _ = deno_core::v8::Platform::new(1, false);
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_scene(
    initial_crdt_store: CrdtStore,
    scene_hash: String,
    scene_js: SceneJsFile,
    crdt_component_interfaces: CrdtComponentInterfaces,
    renderer_sender: SceneResponseSender,
    global_update_receiver: tokio::sync::broadcast::Receiver<GlobalCrdtStateUpdate>,
    id: SceneId,
    storage_root: String,
    inspect: bool,
    testing: bool,
    preview: bool,
    super_user: Option<tokio::sync::mpsc::UnboundedSender<SystemApi>>,
) -> Sender<RendererResponse> {
    let (main_sx, thread_rx) = tokio::sync::mpsc::channel::<RendererResponse>(1);

    std::thread::Builder::new()
        .name(format!("scene thread {:?}", id.0))
        .stack_size(8388608)
        .spawn(move || {
            let thread_result = panic::catch_unwind(AssertUnwindSafe(|| {
                scene_thread(
                    initial_crdt_store,
                    scene_hash,
                    id,
                    storage_root,
                    scene_js,
                    crdt_component_interfaces,
                    renderer_sender,
                    thread_rx,
                    global_update_receiver,
                    inspect,
                    testing,
                    preview,
                    super_user,
                )
            }));

            if let Err(e) = thread_result {
                error!("[{id:?}] caught scene thread panic: {e:?}");
            }
        })
        .unwrap();

    main_sx
}
