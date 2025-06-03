use std::{cell::RefCell, rc::Rc};

use crate::WorkerContext;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub async fn op_get_texture_size(state: &mut WorkerContext, src: String) -> JsValue {
    serde_wasm_bindgen::to_value(
        &dcl::js::adaption_layer_helper::op_get_texture_size(Rc::new(RefCell::new(state)), src)
            .await,
    )
    .unwrap()
}
