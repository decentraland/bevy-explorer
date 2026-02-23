use anyhow::anyhow;
use bevy::{
    log::{debug, error},
    math::f32,
};
use common::{
    rpc::{ReadFileResponse, RpcCall, RpcResultSender},
    structs::TimeOfDay,
};
use dcl_component::{
    proto_components::sdk::components::PbRealmInfo, DclReader, FromDclReader, SceneComponentId,
    SceneEntityId,
};
use serde::Serialize;
use std::{cell::RefCell, rc::Rc};

use crate::{
    interface::{crdt_context::CrdtContext, CrdtType},
    js::{RendererStore, SceneResponseSender},
    RpcCalls, SceneResponse,
};

use super::State;

pub async fn op_read_file(
    op_state: Rc<RefCell<impl State>>,
    filename: String,
) -> Result<ReadFileResponse, anyhow::Error> {
    debug!("op_read_file {filename}");

    let scene_hash = op_state.borrow_mut().borrow::<CrdtContext>().hash.clone();
    let (sx, rx) = RpcResultSender::channel();

    op_state
        .borrow_mut()
        .borrow_mut::<SceneResponseSender>()
        .try_send(SceneResponse::ImmediateRpcCall(RpcCall::ReadFile {
            scene_hash,
            filename,
            response: sx,
        }))
        .unwrap();

    let res = rx.await;

    debug!("op_read_file -> {res:?}");

    res?.map_err(|e| anyhow!(e))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneInfoResponse {
    pub urn: String,
    pub content: Vec<ContentFileEntry>,
    pub metadata_json: String,
    pub base_url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentFileEntry {
    pub file: String,
    pub hash: String,
}

pub async fn op_scene_information(
    op_state: Rc<RefCell<impl State>>,
) -> Result<SceneInfoResponse, anyhow::Error> {
    debug!("op_scene_information");
    scene_information(op_state).await
}

pub async fn scene_information(
    op_state: Rc<RefCell<impl State>>,
) -> Result<SceneInfoResponse, anyhow::Error> {
    let urn = op_state.borrow().borrow::<CrdtContext>().hash.clone();

    let (sx, rx) = RpcResultSender::channel();

    op_state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::EntityDefinition {
            urn: urn.clone(),
            response: sx,
        });

    let entity_definition = rx.await?;

    entity_definition
        .map(|definition| SceneInfoResponse {
            urn,
            content: definition
                .collection
                .into_iter()
                .map(|(file, hash)| ContentFileEntry { file, hash })
                .collect(),
            metadata_json: definition.metadata.unwrap_or_default(),
            base_url: format!("{}/contents/", definition.base_url),
        })
        .ok_or_else(|| anyhow!("Scene hash not found?!"))
}

pub async fn op_realm_information(
    op_state: Rc<RefCell<impl State>>,
) -> Result<PbRealmInfo, anyhow::Error> {
    debug!("op_realm_information");
    realm_information(op_state).await
}

pub async fn realm_information(
    op_state: Rc<RefCell<impl State>>,
) -> Result<PbRealmInfo, anyhow::Error> {
    if let Some(raw_component) = op_state.borrow().borrow::<RendererStore>().0.get(
        SceneComponentId::REALM_INFO,
        CrdtType::LWW_ANY,
        SceneEntityId::ROOT,
    ) {
        return PbRealmInfo::from_reader(&mut DclReader::new(raw_component))
            .map_err(|_| anyhow!("failed to read component"));
    }
    anyhow::bail!("no realm info")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldTime {
    pub time: f32,
}

pub async fn op_world_time(op_state: Rc<RefCell<impl State>>) -> Result<WorldTime, anyhow::Error> {
    debug!("op_world_time");
    let TimeOfDay { time } = op_state.borrow().borrow::<TimeOfDay>();
    Ok(WorldTime { time })
}
