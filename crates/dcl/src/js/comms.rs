use std::{cell::RefCell, rc::Rc};

use common::rpc::RpcCall;
use deno_core::{op, Op, OpDecl, OpState};

use crate::{interface::crdt_context::CrdtContext, RpcCalls};

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![op_comms_send::DECL]
}

#[op]
async fn op_comms_send(state: Rc<RefCell<OpState>>, message: String) {
    let scene = state.borrow().borrow::<CrdtContext>().scene_id.0;
    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::SendMessageBus { scene, message });
}
