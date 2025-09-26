use anyhow::anyhow;
use bevy::{asset::io::AssetReader, log::debug};
use dcl_component::{
    proto_components::sdk::components::PbRealmInfo, DclReader, FromDclReader, SceneComponentId,
    SceneEntityId,
};
use futures_lite::AsyncReadExt;
use ipfs::{
    ipfs_path::{IpfsPath, IpfsType},
    IpfsResource,
};
use serde::Serialize;
use std::{cell::RefCell, path::PathBuf, rc::Rc};

use crate::{
    interface::{crdt_context::CrdtContext, CrdtType},
    js::RendererStore,
};

use super::State;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadFileResponse {
    content: Vec<u8>,
    hash: String,
}

pub async fn op_read_file(
    op_state: Rc<RefCell<impl State>>,
    filename: String,
) -> Result<ReadFileResponse, anyhow::Error> {
    debug!("op_read_file");
    let ipfs = op_state.borrow_mut().borrow::<IpfsResource>().clone();
    let hash = op_state.borrow_mut().borrow::<CrdtContext>().hash.clone();
    let ipfs_path = IpfsPath::new(IpfsType::new_content_file(hash, filename));
    let ipfs_pathbuf = PathBuf::from(&ipfs_path);

    let mut reader = ipfs.read(&ipfs_pathbuf).await.map_err(|e| anyhow!(e))?;
    let hash = ipfs.ipfs_hash(&ipfs_path).await.unwrap_or_default();

    let mut content = Vec::default();
    reader.read_to_end(&mut content).await?;

    Ok(ReadFileResponse { content, hash })
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
    let ipfs = op_state.borrow().borrow::<IpfsResource>().clone();
    ipfs.entity_definition(&urn)
        .await
        .map(|(entity, base_url)| SceneInfoResponse {
            urn,
            content: entity
                .collection
                .values()
                .map(|(k, v)| ContentFileEntry {
                    file: k.to_owned(),
                    hash: v.to_owned(),
                })
                .collect(),
            metadata_json: entity.metadata.unwrap_or_default(),
            base_url: format!("{base_url}/contents/"),
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

    // component not added, fall back to ipfs-based discovery
    let ipfs = op_state.borrow().borrow::<IpfsResource>().clone();
    let (base_url, info) = ipfs.get_realm_info().await;

    let info = info.ok_or_else(|| anyhow!("Not connected?"))?;

    let config = info.configurations.ok_or(anyhow::anyhow!("no realm"))?;
    let realm_name = config.realm_name.unwrap_or_default();
    let base_url = base_url
        .strip_suffix(&format!("/{}", &realm_name))
        .unwrap_or(&base_url);

    let is_preview = op_state.borrow().borrow::<CrdtContext>().preview;

    Ok(PbRealmInfo {
        base_url: base_url.to_owned(),
        realm_name,
        network_id: config.network_id.unwrap_or_default() as i32,
        comms_adapter: info
            .comms
            .as_ref()
            .and_then(|comms| comms.adapter.clone())
            .unwrap_or("offline".to_owned()),
        is_preview,
        room: None,
        is_connected_scene_room: Some(false),
    })
}
