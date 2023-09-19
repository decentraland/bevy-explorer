use bevy::asset::{AssetIo, AssetServer};
use deno_core::{anyhow::anyhow, error::AnyError, op, Op, OpDecl, OpState};
use ipfs::{
    ipfs_path::{IpfsPath, IpfsType},
    IpfsLoaderExt,
};
use serde::Serialize;
use std::{cell::RefCell, path::PathBuf, rc::Rc};

use crate::interface::crdt_context::CrdtContext;

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![op_read_file::DECL, op_scene_information::DECL]
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
    let asset_server = op_state.borrow_mut().borrow::<AssetServer>().clone();
    let hash = op_state.borrow_mut().borrow::<CrdtContext>().hash.clone();
    let ipfs_path = IpfsPath::new(IpfsType::new_content_file(hash, filename));

    let content = asset_server
        .ipfs()
        .load_path(&PathBuf::from(&ipfs_path))
        .await
        .map_err(|e| anyhow!(e))?;
    let hash = asset_server
        .ipfs()
        .ipfs_hash(&ipfs_path)
        .await
        .unwrap_or_default();

    Ok(ReadFileResponse { content, hash })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SceneInfoResponse {
    urn: String,
    content: Vec<ContentFileEntry>,
    meta_data: String,
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
    let asset_server = op_state.borrow().borrow::<AssetServer>().clone();
    asset_server
        .ipfs()
        .entity_definition(&urn)
        .await
        .ok_or_else(|| anyhow!("Scene hash not found?!"))
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
            meta_data: entity.metadata.unwrap_or_default(),
            base_url,
        })
}
