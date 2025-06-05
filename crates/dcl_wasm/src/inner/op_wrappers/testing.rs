use crate::{serde_parse, serde_result, WasmError, WorkerContext};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn op_testing_enabled(op_state: &WorkerContext) -> bool {
    dcl::js::testing::op_testing_enabled(&mut *op_state.state.borrow_mut())
}

#[wasm_bindgen]
pub fn op_log_test_plan(state: &WorkerContext, body: JsValue) {
    serde_parse!(body);
    dcl::js::testing::op_log_test_plan(&mut *state.state.borrow_mut(), body);
}

#[wasm_bindgen]
pub fn op_log_test_result(state: &WorkerContext, body: JsValue) {
    serde_parse!(body);
    dcl::js::testing::op_log_test_result(&mut *state.state.borrow_mut(), body);
}

#[wasm_bindgen]
pub fn op_take_and_compare_snapshot(
    state: &WorkerContext,
    name: String,
    camera_position: JsValue,
    camera_target: JsValue,
    snapshot_size: JsValue,
    method: JsValue,
) -> Result<JsValue, WasmError> {
    serde_parse!(camera_position);
    serde_parse!(camera_target);
    serde_parse!(snapshot_size);
    serde_parse!(method);

    serde_result!(dcl::js::testing::op_take_and_compare_snapshot(
        &mut *state.state.borrow_mut(),
        name,
        camera_position,
        camera_target,
        snapshot_size,
        method,
    ))
}
