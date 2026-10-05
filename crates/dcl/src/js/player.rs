use std::{cell::RefCell, rc::Rc};

use bevy::log::debug;
use common::rpc::{RpcCall, RpcResultSender};

use crate::RpcCalls;

use super::State;

pub async fn op_get_connected_players(
    state: Rc<RefCell<impl State>>,
) -> Result<Vec<String>, anyhow::Error> {
    debug!("op_get_connected_players");
    let (sx, rx) = RpcResultSender::channel();

    {
        let mut state = state.borrow_mut();

        state
            .borrow_mut::<RpcCalls>()
            .push(RpcCall::GetConnectedPlayers { response: sx })?;
    }

    Ok(rx.await.unwrap_or_default())
}

pub async fn op_get_players_in_scene(
    state: Rc<RefCell<impl State>>,
) -> Result<Vec<String>, anyhow::Error> {
    debug!("op_get_players_in_scene");

    let (sx, rx) = RpcResultSender::channel();

    {
        let mut state = state.borrow_mut();

        state
            .borrow_mut::<RpcCalls>()
            .push(RpcCall::GetPlayersInScene { response: sx })?;
    }

    Ok(rx.await.unwrap_or_default())
}
