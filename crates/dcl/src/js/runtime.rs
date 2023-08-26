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
    vec![op_read_file::DECL]
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
