use std::{cell::RefCell, rc::Rc};
use crate::{serde_result, WasmError, WorkerContext};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub async fn op_read_file(
    op_state: &mut WorkerContext,
    filename: String,
) -> Result<JsValue, WasmError> {
    serde_result!(dcl::js::runtime::op_read_file(Rc::new(RefCell::new(op_state)), filename).await)
}

#[wasm_bindgen]
pub async fn op_scene_information(
    op_state: &mut WorkerContext,
) -> Result<JsValue, WasmError> {
    serde_result!(dcl::js::runtime::op_scene_information(Rc::new(RefCell::new(op_state))).await)
}

#[wasm_bindgen]
pub async fn op_realm_information(op_state: &mut WorkerContext) -> Result<JsValue, WasmError> {
    serde_result!(dcl::js::runtime::op_realm_information(Rc::new(RefCell::new(op_state))).await)
}
