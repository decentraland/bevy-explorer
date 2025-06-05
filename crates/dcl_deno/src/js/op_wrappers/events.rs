use dcl::js::events::Event;
use deno_core::{op2, OpDecl, OpState};

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![op_subscribe(), op_unsubscribe(), op_send_batch()]
}

#[op2(fast)]
fn op_subscribe(state: &mut OpState, #[string] id: &str) {
    dcl::js::events::op_subscribe(state, id)
}

#[op2(fast)]
fn op_unsubscribe(state: &mut OpState, #[string] id: &str) {
    dcl::js::events::op_unsubscribe(state, id)
}

#[op2]
#[serde]
fn op_send_batch(state: &mut OpState) -> Vec<Event> {
    dcl::js::events::op_send_batch(state)
}
