use crate::{serde_result, WasmError, WorkerContext};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub async fn op_get_user_data(state: &WorkerContext) -> Result<JsValue, WasmError> {
    serde_result!(dcl::js::user_identity::op_get_user_data(state.rc()).await)
}

#[wasm_bindgen]
pub async fn op_get_player_data(
    state: &WorkerContext,
    id: String,
) -> Result<JsValue, WasmError> {
    serde_result!(dcl::js::user_identity::op_get_player_data(state.rc(), id).await)
}
