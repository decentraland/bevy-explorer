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
use tokio::sync::mpsc::UnboundedSender;

use ipfs::SceneJsFile;

use dcl::{
    interface::{crdt_context::CrdtContext, CrdtComponentInterfaces, CrdtStore},
    js::{KillFlag, SceneResponseSender},
    RendererResponse, SceneId,
};

use crate::js::scene_thread;

pub(crate) static VM_HANDLES: Lazy<Mutex<HashMap<SceneId, (IsolateHandle, KillFlag)>>> =
    Lazy::new(Default::default);

/// interrupt the scene's isolate even if it is stuck in a JS loop, so the scene
/// thread unwinds and exits. no-op if the scene thread has already exited, or
/// for a worker blocked inside a rust op (the termination takes effect when
/// control returns to JS). the kill flag must be set as well as terminating:
/// v8's termination request is consumed once the stack unwinds, and the scene
/// loop doesn't exit on uncaught errors, so without the flag a deterministic
/// wedge would just re-enter its loop on the next tick.
pub fn terminate_scene(scene_id: SceneId) {
    if let Some((handle, kill_flag)) = VM_HANDLES.lock().unwrap().get(&scene_id) {
        bevy::log::warn!("[{scene_id:?}] scene thread still running after kill; force-terminating");
        kill_flag.kill();
        handle.terminate_execution();
    }
}

/// must be called from main thread on linux before any isolates are created
pub fn init_runtime() {
    let _ = deno_core::v8::Platform::new(1, false);
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_scene(
    initial_crdt_store: CrdtStore,
    scene_context: CrdtContext,
    scene_js: SceneJsFile,
    crdt_component_interfaces: CrdtComponentInterfaces,
    renderer_sender: SceneResponseSender,
    global_update_receiver: tokio::sync::broadcast::Receiver<GlobalCrdtStateUpdate>,
    storage_root: String,
    inspect: bool,
    super_user: Option<tokio::sync::mpsc::UnboundedSender<SystemApi>>,
    scene_origin: bevy::prelude::Vec3,
) -> UnboundedSender<RendererResponse> {
    let id = scene_context.scene_id;
    let (main_sx, thread_rx) = tokio::sync::mpsc::unbounded_channel::<RendererResponse>();

    std::thread::Builder::new()
        .name(format!("scene thread {:?}", id.0))
        .stack_size(8388608)
        .spawn(move || {
            let thread_result = panic::catch_unwind(AssertUnwindSafe(|| {
                scene_thread(
                    initial_crdt_store,
                    scene_context,
                    storage_root,
                    scene_js,
                    crdt_component_interfaces,
                    renderer_sender,
                    thread_rx,
                    global_update_receiver,
                    inspect,
                    super_user,
                    scene_origin,
                )
            }));

            if let Err(e) = thread_result {
                error!("[{id:?}] caught scene thread panic: {e:?}");
            }

            VM_HANDLES.lock().unwrap().remove(&id);
        })
        .unwrap();

    main_sx
}
