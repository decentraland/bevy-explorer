use anyhow::anyhow;
use bevy::log::debug;
use common::rpc::{PortableLocation, RpcCall, RpcResultSender, SpawnResponse};
use std::{cell::RefCell, rc::Rc};

use crate::RpcCalls;

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

    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::SpawnPortable {
            location,
            response: sx,
        })?;

    rx.await.map_err(|e| anyhow!(e))?.map_err(|e| anyhow!(e))
}

pub async fn op_portable_kill(
    state: Rc<RefCell<impl State>>,
    pid: String,
) -> Result<bool, anyhow::Error> {
    debug!("op_portable_kill");
    let (sx, rx) = RpcResultSender::channel();

    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::KillPortable {
            location: PortableLocation::Urn(pid.clone()),
            response: sx,
        })?;

    rx.await.map_err(|e| anyhow::anyhow!(e))
}

pub async fn op_portable_list(
    state: Rc<RefCell<impl State>>,
) -> Result<Vec<SpawnResponse>, anyhow::Error> {
    debug!("op_portable_list");
    let (sx, rx) = RpcResultSender::channel();

    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::ListPortables { response: sx })?;

    let res = rx.await.unwrap_or_default();
    bevy::log::debug!("portable list res: {res:?}");
    Ok(res)
}
