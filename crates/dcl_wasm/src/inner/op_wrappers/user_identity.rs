use std::{cell::RefCell, rc::Rc};
use crate::{serde_result, WasmError, WorkerContext};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub async fn op_get_user_data(state: &mut WorkerContext) -> Result<JsValue, WasmError> {
    serde_result!(dcl::js::user_identity::op_get_user_data(Rc::new(RefCell::new(state))).await)
}

#[wasm_bindgen]
pub async fn op_get_player_data(
    state: &mut WorkerContext,
    id: String,
) -> Result<JsValue, WasmError> {
    serde_result!(dcl::js::user_identity::op_get_player_data(Rc::new(RefCell::new(state)), id).await)
}
