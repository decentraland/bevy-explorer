use std::{cell::RefCell, rc::Rc};

use common::rpc::RpcCall;
use deno_core::{op, Op, OpDecl, OpState};

use crate::{interface::crdt_context::CrdtContext, RpcCalls};

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![
        op_get_connected_players::DECL,
        op_get_players_in_scene::DECL,
    ]
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

#[op]
async fn op_get_players_in_scene(state: Rc<RefCell<OpState>>) -> Vec<String> {
    let (sx, rx) = tokio::sync::oneshot::channel::<Vec<String>>();

    let mut state = state.borrow_mut();
    let context = state.borrow::<CrdtContext>();
    let scene = context.scene_id.0;

    state
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::GetPlayersInScene {
            scene,
            response: sx.into(),
        });

    drop(state);

    rx.await.unwrap_or_default()
}
