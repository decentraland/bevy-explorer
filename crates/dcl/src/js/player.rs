use std::{cell::RefCell, rc::Rc};

use common::rpc::{RpcResult, SceneRpcCall};
use deno_core::{op, Op, OpDecl, OpState};

use crate::RpcCalls;

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![op_get_connected_players::DECL]
}

#[op]
async fn op_get_connected_players(state: Rc<RefCell<OpState>>) -> Vec<String> {
    let (sx, rx) = tokio::sync::oneshot::channel::<Vec<String>>();

    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push((SceneRpcCall::GetConnectedPlayers, Some(RpcResult::new(sx))));

    rx.await.unwrap_or_default()
}
