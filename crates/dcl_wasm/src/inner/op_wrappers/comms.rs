use std::{cell::RefCell, rc::Rc};

use crate::{WasmError, WorkerContext};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub async fn op_comms_send_string(state: &mut WorkerContext, message: String) {
    dcl::js::comms::op_comms_send_string(Rc::new(RefCell::new(state)), message).await
}

#[wasm_bindgen]
pub async fn op_comms_send_binary_single(
    state: &mut WorkerContext,
    message: js_sys::ArrayBuffer,
    recipient: String,
) {
    let view = js_sys::Uint8Array::new(&message);
    dcl::js::comms::op_comms_send_binary_single(
        Rc::new(RefCell::new(state)),
        &view.to_vec(),
        recipient,
    )
    .await
}

#[wasm_bindgen]
pub async fn op_comms_recv_binary(state: &mut WorkerContext) -> Result<js_sys::Array, WasmError> {
    let data = dcl::js::comms::op_comms_recv_binary(Rc::new(RefCell::new(state))).await;

    data.map(|r| {
        r.into_iter()
            .map(|v| v.into_boxed_slice())
            .map(JsValue::from)
            .collect()
    })
    .map_err(|e| WasmError::from(e))
}
