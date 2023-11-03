use std::{cell::RefCell, rc::Rc};

use common::rpc::RpcCall;
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
        .push(RpcCall::GetConnectedPlayers {
            response: sx.into(),
        });

    rx.await.unwrap_or_default()
}
