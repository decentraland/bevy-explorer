use crate::{serde_result, WasmError, WorkerContext};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub async fn op_portable_spawn(
    state: &WorkerContext,
    pid: Option<String>,
    ens: Option<String>,
) -> Result<JsValue, WasmError> {
    serde_result!(dcl::js::portables::op_portable_spawn(state.rc(), pid, ens).await)
}

#[wasm_bindgen]
pub async fn op_portable_kill(state: &WorkerContext, pid: String) -> Result<bool, WasmError> {
    dcl::js::portables::op_portable_kill(state.rc(), pid)
        .await
        .map_err(|e| WasmError::from(e))
}

#[wasm_bindgen]
pub async fn op_portable_list(state: &WorkerContext) -> Vec<JsValue> {
    dcl::js::portables::op_portable_list(state.rc())
        .await
        .into_iter()
        .map(|ev| serde_wasm_bindgen::to_value(&ev).unwrap())
        .collect()
}
