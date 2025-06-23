use crate::{WasmError, WorkerContext};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub async fn op_comms_send_string(state: &WorkerContext, message: String) {
    dcl::js::comms::op_comms_send_string(state.rc(), message).await
}

#[wasm_bindgen]
pub async fn op_comms_send_binary_single(
    state: &WorkerContext,
    message: js_sys::ArrayBuffer,
    recipient: Option<String>,
) {
    let view = js_sys::Uint8Array::new(&message);
    dcl::js::comms::op_comms_send_binary_single(state.rc(), &view.to_vec(), recipient).await
}

#[wasm_bindgen]
pub async fn op_comms_recv_binary(state: &WorkerContext) -> Result<js_sys::Array, WasmError> {
    let data = dcl::js::comms::op_comms_recv_binary(state.rc()).await;

    data.map(|r| {
        r.into_iter()
            .map(|v| v.into_boxed_slice())
            .map(JsValue::from)
            .collect()
    })
    .map_err(WasmError::from)
}
