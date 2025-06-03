use std::{cell::RefCell, rc::Rc};
use crate::{WasmError, WorkerContext};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn op_move_player_to(
    op_state: &mut WorkerContext,
    position_x: f32,
    position_y: f32,
    position_z: f32,
    camera: bool,
    maybe_camera_x: f32,
    maybe_camera_y: f32,
    maybe_camera_z: f32,
    looking_at: bool,
    maybe_looking_at_x: f32,
    maybe_looking_at_y: f32,
    maybe_looking_at_z: f32,
) {
    dcl::js::restricted_actions::op_move_player_to(
        op_state,
        position_x,
        position_y,
        position_z,
        camera,
        maybe_camera_x,
        maybe_camera_y,
        maybe_camera_z,
        looking_at,
        maybe_looking_at_x,
        maybe_looking_at_y,
        maybe_looking_at_z,
    );
}

#[wasm_bindgen]
pub async fn op_teleport_to(state: &mut WorkerContext, position_x: i32, position_y: i32) -> bool {
    dcl::js::restricted_actions::op_teleport_to(Rc::new(RefCell::new(state)), position_x, position_y).await
}

#[wasm_bindgen]
pub async fn op_change_realm(
    state: &mut WorkerContext,
    realm: String,
    message: Option<String>,
) -> bool {
    dcl::js::restricted_actions::op_change_realm(Rc::new(RefCell::new(state)), realm, message).await
}

#[wasm_bindgen]
pub async fn op_external_url(state: &mut WorkerContext, url: String) -> bool {
    dcl::js::restricted_actions::op_external_url(Rc::new(RefCell::new(state)), url).await
}

#[wasm_bindgen]
pub fn op_emote(op_state: &mut WorkerContext, emote: String) {
    dcl::js::restricted_actions::op_emote(op_state, emote);
}

#[wasm_bindgen]
pub async fn op_scene_emote(
    op_state: &mut WorkerContext,
    emote: String,
    looping: bool,
) -> Result<(), WasmError> {
    dcl::js::restricted_actions::op_scene_emote(Rc::new(RefCell::new(op_state)), emote, looping).await.map_err(|e| WasmError::from(e))
}

#[wasm_bindgen]
pub async fn op_open_nft_dialog(
    op_state: &mut WorkerContext,
    urn: String,
) -> Result<(), WasmError> {
    dcl::js::restricted_actions::op_open_nft_dialog(Rc::new(RefCell::new(op_state)), urn).await.map_err(|e| WasmError::from(e))
}

#[wasm_bindgen]
pub async fn op_set_ui_focus(
    op_state: &mut WorkerContext,
    element_id: String,
) -> Result<(), WasmError> {
    dcl::js::restricted_actions::op_set_ui_focus(Rc::new(RefCell::new(op_state)), element_id).await.map_err(|e| WasmError::from(e))
}
