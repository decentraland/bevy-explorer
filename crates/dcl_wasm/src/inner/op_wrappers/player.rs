use std::{cell::RefCell, rc::Rc};

use crate::WorkerContext;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub async fn op_get_connected_players(state: &mut WorkerContext) -> Vec<String> {
    dcl::js::player::op_get_connected_players(Rc::new(RefCell::new(state))).await
}

#[wasm_bindgen]
pub async fn op_get_players_in_scene(state: &mut WorkerContext) -> Vec<String> {
    dcl::js::player::op_get_players_in_scene(Rc::new(RefCell::new(state))).await
}
