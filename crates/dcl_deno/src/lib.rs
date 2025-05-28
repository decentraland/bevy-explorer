pub mod js;

use std::{
    panic::{self, AssertUnwindSafe},
    sync::{mpsc::SyncSender, Mutex},
};

use bevy::{log::error, utils::HashMap};
use deno_core::v8::IsolateHandle;
use once_cell::sync::Lazy;
use system_bridge::SystemApi;
use tokio::sync::mpsc::Sender;

use ipfs::{IpfsResource, SceneJsFile};
use wallet::Wallet;

use dcl::{interface::CrdtComponentInterfaces, RendererResponse, SceneId, SceneResponse};

use crate::js::scene_thread;

pub(crate) static VM_HANDLES: Lazy<Mutex<HashMap<SceneId, IsolateHandle>>> =
    Lazy::new(Default::default);

/// must be called from main thread on linux before any isolates are created
pub fn init_runtime() {
    let _ = deno_core::v8::Platform::new(1, false);
}

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
                    scene_hash,
                    id,
                    storage_root,
                    scene_js,
                    crdt_component_interfaces,
                    renderer_sender,
                    thread_rx,
                    global_update_receiver,
                    ipfs,
                    wallet,
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
