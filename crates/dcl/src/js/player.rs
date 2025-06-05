use std::{cell::RefCell, rc::Rc};

use bevy::log::debug;
use common::rpc::RpcCall;

use crate::{interface::crdt_context::CrdtContext, RpcCalls};

use super::State;

pub async fn op_get_connected_players(state: Rc<RefCell<impl State>>) -> Vec<String> {
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

pub async fn op_get_players_in_scene(state: Rc<RefCell<impl State>>) -> Vec<String> {
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
