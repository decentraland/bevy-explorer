use bevy::{platform::collections::HashSet, prelude::Entity};
use common::rpc::{CompareSnapshot, RpcCall};

use dcl_component::{SceneComponentId, SceneEntityId};
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
    /// Component updates plus an engine-initiated census: `died` entities are
    /// deleted scene-side, `born` are reserved for engine-created entities. The
    /// census is sourced from the engine context's `death_row`/`nascent` at the
    /// send point (before the scene's own census is merged in), so it never
    /// echoes the scene's own born/died back to it.
    Ok(CrdtStore, SceneCensus),
    /// Request the scene thread to send back a full clone of its current CRDT state.
    GetCrdtSnapshot,
    /// Allocate `count` fresh entity ids from the scene's allocator (collision-free, correctly
    /// generationed) and instantiate each scene-side by injecting `put_component(id, component_id,
    /// data)` into the receive results — the only way to make the scene's `@dcl/ecs` adopt the
    /// entity. Replies with [`SceneResponse::EntityAllocated`].
    AllocateEntity {
        component_id: SceneComponentId,
        data: Vec<u8>,
        count: usize,
    },
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
    /// Response to [`RendererResponse::GetCrdtSnapshot`]: the full scene-side CRDT state.
    CrdtSnapshot(SceneId, CrdtStore),
    /// Response to [`RendererResponse::AllocateEntity`]: the freshly-allocated entity ids.
    EntityAllocated(SceneId, Vec<SceneEntityId>),
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
