use crate::{serde_result, WasmError, WorkerContext};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub async fn op_send_async(
    state: &WorkerContext,
    method: String,
    params: String,
) -> Result<JsValue, WasmError> {
    serde_result!(dcl::js::ethereum_controller::op_send_async(state.rc(), method, params).await)
}
