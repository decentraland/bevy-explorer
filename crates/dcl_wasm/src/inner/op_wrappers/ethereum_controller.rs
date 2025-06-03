use std::{cell::RefCell, rc::Rc};

use crate::{serde_result, WasmError, WorkerContext};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub async fn op_send_async(
    state: &mut WorkerContext,
    method: String,
    params: String,
) -> Result<JsValue, WasmError> {
    serde_result!(dcl::js::ethereum_controller::op_send_async(Rc::new(RefCell::new(state)), method, params).await)
}
