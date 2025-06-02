// Engine module
use deno_core::{op2, OpDecl, OpState};
use std::{cell::RefCell, rc::Rc};

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![op_crdt_send_to_renderer(), op_crdt_recv_from_renderer()]
}

#[op2(fast)]
fn op_crdt_send_to_renderer(op_state: Rc<RefCell<OpState>>, #[buffer] messages: &[u8]) {
    dcl::js::engine::crdt_send_to_renderer(op_state, messages)
}

#[op2(async)]
#[serde]
async fn op_crdt_recv_from_renderer(op_state: Rc<RefCell<OpState>>) -> Vec<Vec<u8>> {
    dcl::js::engine::op_crdt_recv_from_renderer(op_state).await
}
