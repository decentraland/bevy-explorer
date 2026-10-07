use wasm_bindgen::{prelude::wasm_bindgen, JsValue};

use crate::{serde_result, WasmError, WorkerContext};

#[wasm_bindgen]
pub async fn op_get_active_video_streams(state: &WorkerContext) -> Result<JsValue, WasmError> {
    let data = dcl::js::comms_api::op_get_active_video_streams(state.rc()).await;
    serde_result!(data)
}
