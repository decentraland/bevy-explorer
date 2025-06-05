use crate::WorkerContext;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn op_subscribe(state: &WorkerContext, id: &str) {
    dcl::js::events::op_subscribe(&mut *state.state.borrow_mut(), id)
}

#[wasm_bindgen]
pub fn op_unsubscribe(state: &WorkerContext, id: &str) {
    dcl::js::events::op_unsubscribe(&mut *state.state.borrow_mut(), id)
}

#[wasm_bindgen]
pub fn op_send_batch(state: &WorkerContext) -> js_sys::Array {
    dcl::js::events::op_send_batch(&mut *state.state.borrow_mut())
        .into_iter()
        .map(|ev| serde_wasm_bindgen::to_value(&ev).unwrap())
        .collect()
}
