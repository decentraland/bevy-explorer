use crate::WorkerContext;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn op_subscribe(state: &mut WorkerContext, id: &str) {
    dcl::js::events::op_subscribe(state, id)
}

#[wasm_bindgen]
pub fn op_unsubscribe(state: &mut WorkerContext, id: &str) {
    dcl::js::events::op_unsubscribe(state, id)
}

#[wasm_bindgen]
pub fn op_send_batch(state: &mut WorkerContext) -> js_sys::Array {
    dcl::js::events::op_send_batch(state)
        .into_iter()
        .map(|ev| serde_wasm_bindgen::to_value(&ev).unwrap())
        .collect()
}
