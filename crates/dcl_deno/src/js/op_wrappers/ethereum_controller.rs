use deno_core::{error::AnyError, op2, OpDecl, OpState};
use std::{cell::RefCell, rc::Rc};

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![op_send_async()]
}

#[op2(async)]
#[serde]
async fn op_send_async(
    state: Rc<RefCell<OpState>>,
    #[string] method: String,
    #[string] params: String,
) -> Result<serde_json::Value, AnyError> {
    dcl::js::ethereum_controller::op_send_async(state, method, params).await
}
