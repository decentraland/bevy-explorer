// The scene runtime of a sandbox worker (B). It runs in its own memory,
// with the same dcl_wasm op exports the engine module has, against local channels that the
// engine half (dcl_wasm relay.rs, A) feeds and drains. A calls everything here, wasm to
// wasm, with numbers only: it copies each message into a buffer from `relay_b_alloc` and
// delivers it, and pulls this side's messages with `relay_b_next_*`. The web counterpart of
// dcl_deno_ipc's scene host (scene_ipc_in / scene_ipc_out), for one scene.
#![cfg(target_arch = "wasm32")]

use std::{cell::RefCell, collections::VecDeque, sync::Arc};

use common::{
    rpc::{rmp_encode, CancellationToken, IpcMessage, RequestContext, SCENE_IPC_CONTEXT},
    structs::GlobalCrdtStateUpdate,
};
use dcl::{
    js::{scene_response_channel, KillFlag, SceneResponseReceiver, SceneResponseSender},
    RendererResponse, SceneId,
};
use dcl_wasm::{
    relay_proto::{RelayInit, ToEngine, ToScene},
    SceneInitializationData,
};
use system_bridge::SystemApi;
use tokio::sync::{broadcast, mpsc};
use wasm_bindgen::prelude::*;

struct Scene {
    // dropped on kill, so a scene parked in op_crdt_recv_from_renderer wakes and exits (on
    // main the engine dropping its sender does that; here the engine's sender ends at A)
    thread_sx: Option<mpsc::UnboundedSender<RendererResponse>>,
    global_sx: broadcast::Sender<GlobalCrdtStateUpdate>,
    renderer_rx: SceneResponseReceiver,
    close_rx: mpsc::UnboundedReceiver<u64>,
    // the super-user scene's system api calls, for A to forward
    system_rx: Option<mpsc::UnboundedReceiver<SystemApi>>,
    kill_flag: KillFlag,
    // encoding a local rpc sender spawns its close watcher
    runtime: tokio::runtime::Runtime,
}

thread_local! {
    static SCENE: RefCell<Option<Scene>> = const { RefCell::new(None) };
    static OUTGOING: RefCell<VecDeque<Vec<u8>>> = const { RefCell::new(VecDeque::new()) };
}

fn push(message: &ToEngine) {
    if let Ok(bytes) = rmp_encode(message) {
        OUTGOING.with_borrow_mut(|queue| queue.push_back(bytes));
    }
}

fn init_scene(init: RelayInit) -> bool {
    console_error_panic_hook::set_once();
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread().build() else {
        return false;
    };
    dcl_wasm::init_runtime();

    let (thread_sx, thread_rx) = mpsc::unbounded_channel();
    let (renderer_sender, renderer_rx) = scene_response_channel();
    let (global_sx, global_rx) = broadcast::channel(1000);
    let (close_sx, close_rx) = mpsc::unbounded_channel();
    SCENE_IPC_CONTEXT.set(Some(RequestContext {
        registry: Default::default(),
        close_sender: close_sx,
        next_id: 1,
    }));
    let kill_flag = KillFlag::default();

    let RelayInit {
        initial_crdt_store,
        scene_context,
        scene_js,
        crdt_component_interfaces,
        storage_root,
        scene_origin,
        is_super,
    } = init;
    let (super_user, system_rx) = if is_super {
        let (sx, rx) = mpsc::unbounded_channel();
        (Some(sx), Some(rx))
    } else {
        (None, None)
    };
    let queued = dcl_wasm::queue_scene(SceneInitializationData {
        initial_crdt_store,
        scene_context,
        thread_rx,
        scene_js: ipfs::SceneJsFile(Arc::new(scene_js)),
        crdt_component_interfaces,
        // the relay attaches the scene's id on the engine side; the tag here is dropped
        renderer_sender: SceneResponseSender::new(SceneId::DUMMY, renderer_sender),
        global_update_receiver: global_rx,
        storage_root,
        super_user,
        scene_origin,
        kill_flag: kill_flag.clone(),
        ipc_close: CancellationToken::new(),
    });

    SCENE.set(Some(Scene {
        thread_sx: Some(thread_sx),
        global_sx,
        renderer_rx,
        close_rx,
        system_rx,
        kill_flag,
        runtime,
    }));
    queued
}

/// A buffer for A to copy one message into; ownership passes back with `relay_b_deliver`.
#[wasm_bindgen]
pub fn relay_b_alloc(len: u32) -> u32 {
    Box::into_raw(vec![0u8; len as usize].into_boxed_slice()) as *mut u8 as u32
}

/// Takes back a buffer from `relay_b_alloc` holding one message. 1 if it was handled.
#[wasm_bindgen]
pub fn relay_b_deliver(ptr: u32, len: u32) -> u32 {
    // SAFETY: A passes back exactly the (ptr, len) relay_b_alloc handed out
    let bytes = unsafe {
        Box::from_raw(std::ptr::slice_from_raw_parts_mut(
            ptr as *mut u8,
            len as usize,
        ))
    };
    let Ok(message) = rmp_serde::from_slice::<ToScene>(&bytes) else {
        return 0;
    };
    let handled = match message {
        ToScene::Init(init) => init_scene(*init),
        ToScene::Update(response) => SCENE.with_borrow(|scene| {
            scene
                .as_ref()
                .and_then(|scene| scene.thread_sx.as_ref())
                .is_some_and(|sx| sx.send(response).is_ok())
        }),
        ToScene::Global(update) => SCENE.with_borrow(|scene| {
            // no receivers is not a failure: the scene may not have subscribed yet
            scene
                .as_ref()
                .map(|scene| scene.global_sx.send(update))
                .is_some()
        }),
        ToScene::Ipc(id, IpcMessage::Data(data)) => SCENE_IPC_CONTEXT.with_borrow_mut(|ctx| {
            ctx.as_mut()
                .and_then(|ctx| ctx.registry.get_mut(&id))
                .map(|endpoint| endpoint.send(data))
                .is_some()
        }),
        ToScene::Ipc(id, IpcMessage::Closed) => {
            SCENE_IPC_CONTEXT.with_borrow_mut(|ctx| {
                ctx.as_mut().map(|ctx| ctx.registry.remove(&id));
            });
            true
        }
    };
    handled as u32
}

/// Drains this side's channels into the outgoing queue. Returns the number of messages queued.
#[wasm_bindgen]
pub fn relay_b_pump() -> u32 {
    SCENE.with_borrow_mut(|scene| {
        let Some(scene) = scene.as_mut() else {
            return 0;
        };
        let _runtime = scene.runtime.enter();
        // run the close watchers of senders encoded so far
        scene.runtime.block_on(tokio::task::yield_now());
        let mut moved = 0;
        while let Ok((_, response)) = scene.renderer_rx.try_recv() {
            moved += 1;
            push(&ToEngine::Response(response));
        }
        if let Some(system_rx) = scene.system_rx.as_mut() {
            while let Ok(api) = system_rx.try_recv() {
                moved += 1;
                push(&ToEngine::SystemApi(api));
            }
        }
        while let Ok(id) = scene.close_rx.try_recv() {
            let was_open = SCENE_IPC_CONTEXT.with_borrow_mut(|ctx| {
                ctx.as_mut()
                    .is_some_and(|ctx| ctx.registry.remove(&id).is_some())
            });
            if was_open {
                moved += 1;
                push(&ToEngine::Ipc(id, IpcMessage::Closed));
            }
        }
        moved
    })
}

/// Length of the next outgoing message, 0 if there is none.
#[wasm_bindgen]
pub fn relay_b_next_len() -> u32 {
    OUTGOING.with_borrow(|queue| queue.front().map_or(0, |bytes| bytes.len() as u32))
}

#[wasm_bindgen]
pub fn relay_b_next_ptr() -> u32 {
    OUTGOING.with_borrow(|queue| queue.front().map_or(0, |bytes| bytes.as_ptr() as u32))
}

#[wasm_bindgen]
pub fn relay_b_pop() {
    OUTGOING.with_borrow_mut(|queue| queue.pop_front());
}

/// The engine has dropped the scene (A saw its kill flag): end the scene loop.
#[wasm_bindgen]
pub fn relay_b_kill() {
    SCENE.with_borrow_mut(|scene| {
        if let Some(scene) = scene {
            scene.kill_flag.kill();
            scene.thread_sx = None;
        }
    });
}
