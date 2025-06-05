use crate::WorkerContext;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub async fn op_get_texture_size(state: &WorkerContext, src: String) -> JsValue {
    serde_wasm_bindgen::to_value(
        &dcl::js::adaption_layer_helper::op_get_texture_size(state.rc(), src).await,
    )
    .unwrap()
}
