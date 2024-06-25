use bevy::log::debug;
use common::rpc::{PortableLocation, RpcCall, SpawnResponse};
use deno_core::{
    anyhow::{self, anyhow},
    error::AnyError,
    op2, OpDecl, OpState,
};
use std::{cell::RefCell, rc::Rc};

use crate::interface::crdt_context::CrdtContext;

use super::RpcCalls;

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![op_portable_spawn(), op_portable_list(), op_portable_kill()]
}

#[op2(async)]
#[serde]
async fn op_portable_spawn(
    state: Rc<RefCell<OpState>>,
    #[string] pid: Option<String>,
    #[string] ens: Option<String>,
) -> Result<SpawnResponse, AnyError> {
    debug!("op_portable_spawn");
    let (sx, rx) = tokio::sync::oneshot::channel::<Result<SpawnResponse, String>>();

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
            response: sx.into(),
        });

    rx.await
        .map_err(|e| anyhow::anyhow!(e))?
        .map_err(|e| anyhow!(e))
}

#[op2(async)]
async fn op_portable_kill(
    state: Rc<RefCell<OpState>>,
    #[string] pid: String,
) -> Result<bool, AnyError> {
    debug!("op_portable_kill");
    let (sx, rx) = tokio::sync::oneshot::channel::<bool>();

    let scene = state.borrow().borrow::<CrdtContext>().scene_id.0;

    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::KillPortable {
            scene,
            location: PortableLocation::Urn(pid.clone()),
            response: sx.into(),
        });

    rx.await.map_err(|e| anyhow::anyhow!(e))
}

#[op2(async)]
#[serde]
async fn op_portable_list(state: Rc<RefCell<OpState>>) -> Vec<SpawnResponse> {
    debug!("op_portable_list");
    let (sx, rx) = tokio::sync::oneshot::channel::<Vec<SpawnResponse>>();

    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::ListPortables {
            response: sx.into(),
        });

    let res = rx.await.unwrap_or_default();
    bevy::utils::tracing::debug!("portable list res: {res:?}");
    res
}
