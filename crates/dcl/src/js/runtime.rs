use bevy::asset::io::AssetReader;
use deno_core::{anyhow::anyhow, error::AnyError, futures::AsyncReadExt, op, Op, OpDecl, OpState};
use ipfs::{
    ipfs_path::{IpfsPath, IpfsType},
    IpfsResource,
};
use serde::Serialize;
use std::{cell::RefCell, path::PathBuf, rc::Rc};

use crate::interface::crdt_context::CrdtContext;

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![
        op_read_file::DECL,
        op_scene_information::DECL,
        op_realm_information::DECL,
    ]
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReadFileResponse {
    content: Vec<u8>,
    hash: String,
}

#[op(v8)]
async fn op_read_file(
    op_state: Rc<RefCell<OpState>>,
    filename: String,
) -> Result<ReadFileResponse, AnyError> {
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
struct SceneInfoResponse {
    urn: String,
    content: Vec<ContentFileEntry>,
    metadata_json: String,
    base_url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ContentFileEntry {
    file: String,
    hash: String,
}

#[op]
async fn op_scene_information(
    op_state: Rc<RefCell<OpState>>,
) -> Result<SceneInfoResponse, AnyError> {
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
            base_url: format!("{}/contents/", base_url),
        })
        .ok_or_else(|| anyhow!("Scene hash not found?!"))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RealmInfoResponse {
    base_url: String,
    realm_name: String,
    network_id: u32,
    comms_adapter: String,
    is_preview: bool,
}

#[op]
async fn op_realm_information(
    op_state: Rc<RefCell<OpState>>,
) -> Result<RealmInfoResponse, AnyError> {
    let ipfs = op_state.borrow().borrow::<IpfsResource>().clone();
    let (base_url, info) = ipfs.get_realm_info().await;

    let info = info.ok_or_else(|| anyhow!("Not connected?"))?;

    let base_url = base_url.strip_suffix("/content").unwrap_or(&base_url);
    let config = info.configurations.unwrap_or_default();

    Ok(RealmInfoResponse {
        base_url: base_url.to_owned(),
        realm_name: config.realm_name.unwrap_or_default(),
        network_id: config.network_id.unwrap_or_default(),
        comms_adapter: info.comms.and_then(|c| c.fixed_adapter).unwrap_or_default(),
        is_preview: false,
    })
}
