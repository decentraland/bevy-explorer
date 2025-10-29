pub mod gotham_state;
pub mod local_storage;
pub mod op_wrappers;

use std::{
    cell::RefCell,
    rc::Rc,
    sync::{mpsc::SyncSender, Arc},
};

use bevy::{log::tracing::span::EnteredSpan, tasks::IoTaskPool};
use dcl::{
    interface::CrdtComponentInterfaces,
    js::{CommunicatedWithRenderer, ShuttingDown, SuperUserScene},
    RendererResponse, SceneId, SceneResponse,
};
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
    if SCENE_QUEUE.set(Default::default()).is_err() {
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
    let scene_initialization_data: SceneInitializationData = SCENE_QUEUE
        .get()
        .expect("scene queue not initialized")
        .lock()
        .await
        .pop()
        .unwrap();
    let context = WorkerContext {
        state: Default::default(),
    };

    dcl::js::init_state(
        &mut *context.state.borrow_mut(),
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

    local_storage::init(&context).await;

    Ok(context)
}

#[wasm_bindgen]
pub struct WorkerContext {
    state: Rc<RefCell<GothamState>>,
}

#[wasm_bindgen]
impl WorkerContext {
    pub fn get_source(&self) -> JsValue {
        (*self.state.borrow().borrow::<SceneJsFile>().0)
            .clone()
            .into()
    }

    pub(crate) fn rc(&self) -> Rc<RefCell<GothamState>> {
        self.state.clone()
    }
}

impl dcl::js::State for GothamState {
    fn borrow<T: 'static>(&self) -> &T {
        self.borrow()
    }

    fn try_borrow<T: 'static>(&self) -> Option<&T> {
        self.try_borrow()
    }

    fn borrow_mut<T: 'static>(&mut self) -> &mut T {
        self.borrow_mut()
    }

    fn try_borrow_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.try_borrow_mut()
    }

    fn has<T: 'static>(&self) -> bool {
        self.has::<T>()
    }

    fn put<T: 'static>(&mut self, value: T) {
        self.put(value)
    }

    fn take<T: 'static>(&mut self) -> T {
        self.take()
    }

    fn try_take<T: 'static>(&mut self) -> Option<T> {
        self.try_take()
    }
}

#[macro_export]
macro_rules! serde_parse {
    ($js_value: ident) => {
        let $js_value = serde_wasm_bindgen::from_value($js_value).unwrap();
    };
}

#[macro_export]
macro_rules! serde_result {
    ($code: expr) => {
        $code
            .map(|v| serde_wasm_bindgen::to_value(&v).unwrap())
            .map_err(WasmError::from)
    };
}

pub struct WasmError(anyhow::Error);

impl From<anyhow::Error> for WasmError {
    fn from(value: anyhow::Error) -> Self {
        Self(value)
    }
}

impl From<WasmError> for JsValue {
    fn from(value: WasmError) -> Self {
        js_sys::Error::new(&value.0.to_string()).into()
    }
}

#[wasm_bindgen]
pub fn op_continue_running(state: &WorkerContext) -> bool {
    !state.state.borrow().has::<ShuttingDown>()
}

#[wasm_bindgen]
pub fn op_communicated_with_renderer(state: &WorkerContext) -> bool {
    state
        .state
        .borrow_mut()
        .try_take::<CommunicatedWithRenderer>()
        .is_some()
}

#[wasm_bindgen]
pub fn drop_context(state: WorkerContext) {
    let span = state.state.borrow_mut().try_take::<EnteredSpan>();
    drop(span);
    let strong_count = Rc::strong_count(&state.state);
    let weak_count = Rc::strong_count(&state.state);

    let Ok(inner) = Rc::try_unwrap(state.state) else {
        panic!("strong: {strong_count}, weak: {weak_count}");
    };

    let inner_inner = inner.into_inner();
    drop(inner_inner);
}

#[wasm_bindgen]
pub fn builtin_module(state: &WorkerContext, path: &str) -> Result<String, String> {
    match path {
        // system api (only allowed for su scene)
        "~system/BevyExplorerApi" => {
            if state
                .state
                .borrow()
                .try_borrow::<SuperUserScene>()
                .is_some()
            {
                Ok(include_str!("../../../dcl/src/js/modules/SystemApi.js").to_owned())
            } else {
                Err(format!("invalid module request `{path}`"))
            }
        }
        // core module load
        "~system/CommunicationsController" => {
            Ok(include_str!("../../../dcl/src/js/modules/CommunicationsController.js").to_owned())
        }
        "~system/CommsApi" => {
            Ok(include_str!("../../../dcl/src/js/modules/CommsApi.js").to_owned())
        }
        "~system/EngineApi" => {
            Ok(include_str!("../../../dcl/src/js/modules/EngineApi.js").to_owned())
        }
        "~system/EnvironmentApi" => {
            Ok(include_str!("../../../dcl/src/js/modules/EnvironmentApi.js").to_owned())
        }
        "~system/EthereumController" => {
            Ok(include_str!("../../../dcl/src/js/modules/EthereumController.js").to_owned())
        }
        "~system/Players" => Ok(include_str!("../../../dcl/src/js/modules/Players.js").to_owned()),
        "~system/PortableExperiences" => {
            Ok(include_str!("../../../dcl/src/js/modules/PortableExperiences.js").to_owned())
        }
        "~system/RestrictedActions" => {
            Ok(include_str!("../../../dcl/src/js/modules/RestrictedActions.js").to_owned())
        }
        "~system/Runtime" => Ok(include_str!("../../../dcl/src/js/modules/Runtime.js").to_owned()),
        "~system/Scene" => Ok(include_str!("../../../dcl/src/js/modules/Scene.js").to_owned()),
        "~system/SignedFetch" => {
            Ok(include_str!("../../../dcl/src/js/modules/SignedFetch.js").to_owned())
        }
        "~system/Testing" => Ok(include_str!("../../../dcl/src/js/modules/Testing.js").to_owned()),
        "~system/UserActionModule" => {
            Ok(include_str!("../../../dcl/src/js/modules/UserActionModule.js").to_owned())
        }
        "~system/UserIdentity" => {
            Ok(include_str!("../../../dcl/src/js/modules/UserIdentity.js").to_owned())
        }
        "~system/AdaptationLayerHelper" => {
            Ok(include_str!("../../../dcl/src/js/modules/AdaptationLayerHelper.js").to_owned())
        }
        _ => Err(format!("invalid module request `{path}`")),
    }
}

#[wasm_bindgen]
pub fn is_super(state: &WorkerContext) -> bool {
    state
        .state
        .borrow()
        .try_borrow::<SuperUserScene>()
        .is_some()
}
