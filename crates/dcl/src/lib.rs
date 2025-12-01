use bevy::{platform::collections::HashSet, prelude::Entity};
use common::rpc::{CompareSnapshot, RpcCall};

use dcl_component::SceneEntityId;
use serde::{Deserialize, Serialize};

use self::interface::{CrdtComponentInterfaces, CrdtStore};

pub mod crdt;
pub mod interface;
pub mod js;

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SceneId(pub Entity);

impl SceneId {
    pub const DUMMY: SceneId = SceneId(Entity::PLACEHOLDER);
}

// message from scene describing new and deleted entities
#[derive(Debug, Serialize, Deserialize)]
pub struct SceneCensus {
    pub scene_id: SceneId,
    pub born: HashSet<SceneEntityId>,
    pub died: HashSet<SceneEntityId>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SceneElapsedTime(pub f32);

// data from renderer to scene
#[derive(Debug, Serialize, Deserialize)]
pub enum RendererResponse {
    Ok(CrdtStore),
}

pub type RpcCalls = Vec<RpcCall>;

#[allow(clippy::large_enum_variant)] // we don't care since the error case is very rare
// data from scene to renderer
#[derive(Debug, Serialize, Deserialize)]
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
    ImmediateRpcCall(RpcCall),
    WaitingForInspector,
    CompareSnapshot(CompareSnapshot),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SceneLogLevel {
    Log,
    SceneError,
    SystemError,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SceneLogMessage {
    pub timestamp: f64, // scene local time
    pub level: SceneLogLevel,
    pub message: String,
}
