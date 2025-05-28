use common::rpc::SpawnResponse;
use deno_core::{error::AnyError, op2, OpDecl, OpState};
use std::{cell::RefCell, rc::Rc};

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
    dcl::js::portables::op_portable_spawn(state, pid, ens).await
}

#[op2(async)]
async fn op_portable_kill(
    state: Rc<RefCell<OpState>>,
    #[string] pid: String,
) -> Result<bool, AnyError> {
    dcl::js::portables::op_portable_kill(state, pid).await
}

#[op2(async)]
#[serde]
async fn op_portable_list(state: Rc<RefCell<OpState>>) -> Vec<SpawnResponse> {
    dcl::js::portables::op_portable_list(state).await
}
