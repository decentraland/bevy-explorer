use crate::{serde_result, WasmError, WorkerContext};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub async fn op_get_texture_size(state: &WorkerContext, src: String) -> Result<JsValue, WasmError> {
    serde_result!(dcl::js::adaption_layer_helper::op_get_texture_size(state.rc(), src).await)
}
