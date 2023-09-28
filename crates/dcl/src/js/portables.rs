use common::rpc::{PortableLocation, RpcResult, SceneRpcCall, SpawnResponse};
use deno_core::{
    anyhow::{self, anyhow},
    error::AnyError,
    op, Op, OpDecl, OpState,
};
use std::{cell::RefCell, rc::Rc};

use super::RpcCalls;

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![
        op_portable_spawn::DECL,
        op_portable_list::DECL,
        op_portable_kill::DECL,
    ]
}

#[op]
async fn op_portable_spawn(
    state: Rc<RefCell<OpState>>,
    pid: Option<String>,
    ens: Option<String>,
) -> Result<SpawnResponse, AnyError> {
    let (sx, rx) = tokio::sync::oneshot::channel::<Result<SpawnResponse, String>>();

    let location = match (pid, ens) {
        (Some(urn), None) => PortableLocation::Urn(urn.clone()),
        (None, Some(ens)) => PortableLocation::Ens(ens.clone()),
        _ => anyhow::bail!("provide exactly one of `pid` and `ens`"),
    };

    state.borrow_mut().borrow_mut::<RpcCalls>().push((
        SceneRpcCall::SpawnPortable { location },
        Some(RpcResult::new(sx)),
    ));

    rx.await
        .map_err(|e| anyhow::anyhow!(e))?
        .map_err(|e| anyhow!(e))
}

#[op]
async fn op_portable_kill(state: Rc<RefCell<OpState>>, pid: String) -> Result<bool, AnyError> {
    let (sx, rx) = tokio::sync::oneshot::channel::<bool>();

    // might not be a urn, who even knows

    state.borrow_mut().borrow_mut::<RpcCalls>().push((
        SceneRpcCall::KillPortable {
            location: PortableLocation::Urn(pid.clone()),
        },
        Some(RpcResult::new(sx)),
    ));

    rx.await.map_err(|e| anyhow::anyhow!(e))
}

#[op]
async fn op_portable_list(state: Rc<RefCell<OpState>>) -> Vec<SpawnResponse> {
    let (sx, rx) = tokio::sync::oneshot::channel::<Vec<SpawnResponse>>();

    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push((SceneRpcCall::ListPortables, Some(RpcResult::new(sx))));

    let res = rx.await.unwrap_or_default();
    println!("res: {res:?}");
    res
}
