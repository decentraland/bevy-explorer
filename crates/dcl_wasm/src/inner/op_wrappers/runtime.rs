use crate::{serde_result, WasmError, WorkerContext};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub async fn op_read_file(
    op_state: &WorkerContext,
    filename: String,
) -> Result<JsValue, WasmError> {
    serde_result!(dcl::js::runtime::op_read_file(op_state.rc(), filename).await)
}

#[wasm_bindgen]
pub async fn op_scene_information(op_state: &WorkerContext) -> Result<JsValue, WasmError> {
    serde_result!(dcl::js::runtime::op_scene_information(op_state.rc()).await)
}

#[wasm_bindgen]
pub async fn op_realm_information(op_state: &WorkerContext) -> Result<JsValue, WasmError> {
    serde_result!(dcl::js::runtime::op_realm_information(op_state.rc()).await)
}

#[wasm_bindgen]
pub async fn op_world_time(op_state: &WorkerContext) -> Result<JsValue, WasmError> {
    serde_result!(dcl::js::runtime::op_world_time(op_state.rc()).await)
}
