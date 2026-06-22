use bevy::{platform::collections::HashSet, prelude::Entity};
use common::rpc::{CompareSnapshot, RpcCall};

use dcl_component::{proto_components::common::Color3, SceneComponentId, SceneEntityId};
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
    ///
    /// When `explicit_ids` is `Some`, those exact ids (proto-u32 form) are instantiated instead of
    /// freshly allocated — used to recreate entities at their original ids on a freshly-reloaded
    /// scene (where the id sits at its original generation and is free). `count` is ignored in that
    /// case. A requested id that is currently alive is a collision and fails the request.
    AllocateEntity {
        component_id: SceneComponentId,
        data: Vec<u8>,
        count: usize,
        explicit_ids: Option<Vec<u32>>,
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
    /// Response to [`RendererResponse::AllocateEntity`]: one result per requested slot, in order —
    /// `Ok(id)` for an instantiated entity, `Err` for a slot that couldn't be allocated (an explicit
    /// id that was already live, or no free id for a fresh allocation).
    EntityAllocated(SceneId, Vec<Result<SceneEntityId, AllocError>>),
}

/// Why an [`RendererResponse::AllocateEntity`] slot couldn't be allocated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AllocError {
    /// The requested explicit id was already live (a collision).
    Collision(SceneEntityId),
    /// No free id was available for a fresh allocation.
    NoFreeId,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ClearableColor3 {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub clear: bool,
}

impl ClearableColor3 {
    pub fn to_color3(self) -> Option<Color3> {
        if self.clear {
            None
        } else {
            Some(Color3 {
                r: self.r,
                g: self.g,
                b: self.b,
            })
        }
    }
}
