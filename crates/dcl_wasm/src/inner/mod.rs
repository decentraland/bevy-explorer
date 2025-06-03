pub mod gotham_state;
pub mod op_wrappers;

use std::sync::{mpsc::SyncSender, Arc};

use bevy::tasks::IoTaskPool;
use dcl::{interface::CrdtComponentInterfaces, RendererResponse, SceneId, SceneResponse};
use gotham_state::GothamState;
use ipfs::{IpfsResource, SceneJsFile};
use once_cell::sync::OnceCell;
use system_bridge::SystemApi;
use tokio::sync::{
    mpsc::{channel, Receiver, Sender},
    Mutex,
};
use wallet::Wallet;

pub struct SceneInitializationData {
    pub thread_rx: Receiver<RendererResponse>,
    pub scene_hash: String,
    pub scene_js: SceneJsFile,
    pub crdt_component_interfaces: CrdtComponentInterfaces,
    pub renderer_sender: SyncSender<SceneResponse>,
    pub global_update_receiver: tokio::sync::broadcast::Receiver<Vec<u8>>,
    pub ipfs: IpfsResource,
    pub wallet: Wallet,
    pub id: SceneId,
    pub storage_root: String,
    pub inspect: bool,
    pub testing: bool,
    pub preview: bool,
    pub super_user: Option<tokio::sync::mpsc::UnboundedSender<SystemApi>>,
}

// Static storage shared data
static SCENE_QUEUE: OnceCell<Arc<Mutex<Vec<SceneInitializationData>>>> = OnceCell::new();

pub fn init_runtime() {
    if let Err(_) = SCENE_QUEUE.set(Default::default()) {
        panic!("can't init wasm queue");
    }
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
    // create engine channel
    let (thread_sx, thread_rx) = channel(1);

    IoTaskPool::get()
        .spawn(async move {
            // push data to static vec
            SCENE_QUEUE
                .get()
                .unwrap()
                .lock()
                .await
                .push(SceneInitializationData {
                    thread_rx,
                    scene_hash,
                    scene_js,
                    crdt_component_interfaces,
                    renderer_sender,
                    global_update_receiver,
                    ipfs,
                    wallet,
                    id,
                    storage_root,
                    inspect,
                    testing,
                    preview,
                    super_user,
                });

            // spin up a scene thread to consume it
            spawn_and_init_sandbox().await
        })
        .detach();

    thread_sx
}

use wasm_bindgen::prelude::*;

// This block imports the global JS function we defined in main.js
#[wasm_bindgen(js_namespace = window)]
extern "C" {
    #[wasm_bindgen(js_name = spawn_and_init_sandbox)]
    async fn spawn_and_init_sandbox();
}

#[wasm_bindgen]
pub async fn wasm_init_scene() -> Result<WorkerContext, JsValue> {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    let _ = console_log::init_with_level(log::Level::Info);

    let scene_initialization_data: SceneInitializationData =
        SCENE_QUEUE.get().unwrap().lock().await.pop().unwrap();
    let mut context = WorkerContext {
        state: Default::default(),
    };

    dcl::js::init_state(
        &mut &mut context,
        scene_initialization_data.scene_hash,
        scene_initialization_data.id,
        scene_initialization_data.storage_root,
        scene_initialization_data.scene_js,
        scene_initialization_data.crdt_component_interfaces,
        scene_initialization_data.renderer_sender,
        scene_initialization_data.thread_rx,
        scene_initialization_data.global_update_receiver,
        scene_initialization_data.ipfs,
        scene_initialization_data.wallet,
        scene_initialization_data.inspect,
        scene_initialization_data.testing,
        scene_initialization_data.preview,
        scene_initialization_data.super_user,
    );

    Ok(context)
}

#[wasm_bindgen]
pub struct WorkerContext {
    state: GothamState,
}

#[wasm_bindgen]
impl WorkerContext {
    pub fn get_source(&self) -> JsValue {
        (*self.state.borrow::<SceneJsFile>().0).clone().into()
    }
}

impl dcl::js::State for &mut WorkerContext {
    fn borrow<T: 'static>(&self) -> &T {
        self.state.borrow()
    }

    fn try_borrow<T: 'static>(&self) -> Option<&T> {
        self.state.try_borrow()
    }

    fn borrow_mut<T: 'static>(&mut self) -> &mut T {
        self.state.borrow_mut()
    }

    fn try_borrow_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.state.try_borrow_mut()
    }

    fn has<T: 'static>(&self) -> bool {
        self.state.has::<T>()
    }

    fn put<T: 'static>(&mut self, value: T) {
        self.state.put(value)
    }

    fn take<T: 'static>(&mut self) -> T {
        self.state.take()
    }

    fn try_take<T: 'static>(&mut self) -> Option<T> {
        self.state.try_take()
    }
}

impl dcl::js::State for WorkerContext {
    fn borrow<T: 'static>(&self) -> &T {
        self.state.borrow()
    }

    fn try_borrow<T: 'static>(&self) -> Option<&T> {
        self.state.try_borrow()
    }

    fn borrow_mut<T: 'static>(&mut self) -> &mut T {
        self.state.borrow_mut()
    }

    fn try_borrow_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.state.try_borrow_mut()
    }

    fn has<T: 'static>(&self) -> bool {
        self.state.has::<T>()
    }

    fn put<T: 'static>(&mut self, value: T) {
        self.state.put(value)
    }

    fn take<T: 'static>(&mut self) -> T {
        self.state.take()
    }

    fn try_take<T: 'static>(&mut self) -> Option<T> {
        self.state.try_take()
    }
}    
