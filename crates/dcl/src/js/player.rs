use std::{cell::RefCell, rc::Rc};

use bevy::log::debug;
use common::rpc::RpcCall;
use deno_core::{op2, OpDecl, OpState};

use crate::{interface::crdt_context::CrdtContext, RpcCalls};

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![op_get_connected_players(), op_get_players_in_scene()]
}

#[op2(async)]
#[serde]
async fn op_get_connected_players(state: Rc<RefCell<OpState>>) -> Vec<String> {
    debug!("op_get_connected_players");
    let (sx, rx) = tokio::sync::oneshot::channel::<Vec<String>>();

    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::GetConnectedPlayers {
            response: sx.into(),
        });

    rx.await.unwrap_or_default()
}

#[op2(async)]
#[serde]
async fn op_get_players_in_scene(state: Rc<RefCell<OpState>>) -> Vec<String> {
    debug!("op_get_players_in_scene");

    let (sx, rx) = tokio::sync::oneshot::channel::<Vec<String>>();

    {
        let mut state = state.borrow_mut();
        let context = state.borrow::<CrdtContext>();
        let scene = context.scene_id.0;

        state
            .borrow_mut::<RpcCalls>()
            .push(RpcCall::GetPlayersInScene {
                scene,
                response: sx.into(),
            });
    }

    rx.await.unwrap_or_default()
}
