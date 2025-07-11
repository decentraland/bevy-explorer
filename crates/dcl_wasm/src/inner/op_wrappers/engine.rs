use crate::WorkerContext;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn op_crdt_send_to_renderer(op_state: &WorkerContext, messages: js_sys::ArrayBuffer) {
    let view = js_sys::Uint8Array::new(&messages);
    dcl::js::engine::crdt_send_to_renderer(op_state.rc(), &view.to_vec())
}

#[wasm_bindgen]
pub async fn op_crdt_recv_from_renderer(op_state: &WorkerContext) -> js_sys::Array {
    let data = dcl::js::engine::op_crdt_recv_from_renderer(op_state.rc()).await;

    data.into_iter()
        .map(|v| v.into_boxed_slice())
        .map(JsValue::from)
        .collect()
}
