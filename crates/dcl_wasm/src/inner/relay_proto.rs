// The messages between the engine half of the scene relay (relay.rs, A) and the scene
// runtime (crates/dcl_scene_wasm, B). The web counterpart of dcl_deno_ipc's EngineToScene /
// SceneToEngine, for one scene.

use common::{rpc::IpcMessage, structs::GlobalCrdtStateUpdate};
use dcl::{
    interface::{crdt_context::CrdtContext, CrdtComponentInterfaces, CrdtStore},
    RendererResponse, SceneResponse,
};
use serde::{Deserialize, Serialize};
use system_bridge::SystemApi;

#[derive(Serialize, Deserialize)]
pub struct RelayInit {
    pub initial_crdt_store: CrdtStore,
    pub scene_context: CrdtContext,
    pub scene_js: String,
    pub crdt_component_interfaces: CrdtComponentInterfaces,
    pub storage_root: String,
    pub scene_origin: bevy::prelude::Vec3,
    // the scene gets a SystemApi sender; A forwards only if it holds the engine's
    pub is_super: bool,
}

#[derive(Serialize, Deserialize)]
pub enum ToScene {
    Init(Box<RelayInit>),
    Update(RendererResponse),
    Global(GlobalCrdtStateUpdate),
    Ipc(u64, IpcMessage),
}

#[derive(Serialize, Deserialize)]
pub enum ToEngine {
    Response(SceneResponse),
    SystemApi(SystemApi),
    Ipc(u64, IpcMessage),
}
