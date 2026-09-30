use crate::{WasmError, WorkerContext};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub async fn op_get_connected_players(state: &WorkerContext) -> Result<Vec<String>, WasmError> {
    dcl::js::player::op_get_connected_players(state.rc())
        .await
        .map_err(WasmError::from)
}

#[wasm_bindgen]
pub async fn op_get_players_in_scene(state: &WorkerContext) -> Result<Vec<String>, WasmError> {
    dcl::js::player::op_get_players_in_scene(state.rc())
        .await
        .map_err(WasmError::from)
}
