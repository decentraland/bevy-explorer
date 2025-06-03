use crate::WorkerContext;
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn op_crdt_send_to_renderer(op_state: &mut WorkerContext, messages: js_sys::ArrayBuffer) {
    let view = js_sys::Uint8Array::new(&messages);
    dcl::js::engine::crdt_send_to_renderer(Rc::new(RefCell::new(op_state)), &view.to_vec())
}

#[wasm_bindgen]
pub async fn op_crdt_recv_from_renderer(op_state: &mut WorkerContext) -> js_sys::Array {
    let data = dcl::js::engine::op_crdt_recv_from_renderer(Rc::new(RefCell::new(op_state))).await;

    data.into_iter()
        .map(|v| v.into_boxed_slice())
        .map(JsValue::from)
        .collect()
}
