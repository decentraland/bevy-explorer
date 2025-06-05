use crate::WorkerContext;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub async fn op_get_connected_players(state: &WorkerContext) -> Vec<String> {
    dcl::js::player::op_get_connected_players(state.rc()).await
}

#[wasm_bindgen]
pub async fn op_get_players_in_scene(state: &WorkerContext) -> Vec<String> {
    dcl::js::player::op_get_players_in_scene(state.rc()).await
}
