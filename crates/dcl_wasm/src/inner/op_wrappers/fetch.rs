use crate::{serde_result, WasmError, WorkerContext};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub async fn op_signed_fetch_headers(
    state: &WorkerContext,
    uri: String,
    method: Option<String>,
) -> Result<JsValue, WasmError> {
    serde_result!(dcl::js::fetch::op_signed_fetch_headers(state.rc(), uri, method).await)
}
