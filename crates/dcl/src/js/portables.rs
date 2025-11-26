use anyhow::anyhow;
use bevy::log::debug;
use common::rpc::{PortableLocation, RpcCall, RpcResultSender, SpawnResponse};
use std::{cell::RefCell, rc::Rc};

use crate::{interface::crdt_context::CrdtContext, RpcCalls};

use super::State;

pub async fn op_portable_spawn(
    state: Rc<RefCell<impl State>>,
    pid: Option<String>,
    ens: Option<String>,
) -> Result<SpawnResponse, anyhow::Error> {
    debug!("op_portable_spawn");
    let (sx, rx) = RpcResultSender::channel();

    let location = match (pid, ens) {
        (Some(urn), None) => PortableLocation::Urn(urn.clone()),
        (None, Some(ens)) => PortableLocation::Ens(ens.clone()),
        _ => anyhow::bail!("provide exactly one of `pid` and `ens`"),
    };

    let scene = state.borrow().borrow::<CrdtContext>().scene_id.0;

    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::SpawnPortable {
            location,
            spawner: scene,
            response: sx,
        });

    rx.await.map_err(|e| anyhow!(e))?.map_err(|e| anyhow!(e))
}

pub async fn op_portable_kill(
    state: Rc<RefCell<impl State>>,
    pid: String,
) -> Result<bool, anyhow::Error> {
    debug!("op_portable_kill");
    let (sx, rx) = RpcResultSender::channel();

    let scene = state.borrow().borrow::<CrdtContext>().scene_id.0;

    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::KillPortable {
            scene,
            location: PortableLocation::Urn(pid.clone()),
            response: sx,
        });

    rx.await.map_err(|e| anyhow::anyhow!(e))
}

pub async fn op_portable_list(state: Rc<RefCell<impl State>>) -> Vec<SpawnResponse> {
    debug!("op_portable_list");
    let (sx, rx) = RpcResultSender::channel();

    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::ListPortables {
            response: sx,
        });

    let res = rx.await.unwrap_or_default();
    bevy::log::debug!("portable list res: {res:?}");
    res
}
