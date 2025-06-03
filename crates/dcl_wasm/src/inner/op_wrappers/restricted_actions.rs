use crate::{WasmError, WorkerContext};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn op_move_player_to(
    op_state: &WorkerContext,
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
        &mut *op_state.state.borrow_mut(),
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
pub async fn op_teleport_to(state: &WorkerContext, position_x: i32, position_y: i32) -> bool {
    dcl::js::restricted_actions::op_teleport_to(state.rc(), position_x, position_y).await
}

#[wasm_bindgen]
pub async fn op_change_realm(
    state: &WorkerContext,
    realm: String,
    message: Option<String>,
) -> bool {
    dcl::js::restricted_actions::op_change_realm(state.rc(), realm, message).await
}

#[wasm_bindgen]
pub async fn op_external_url(state: &WorkerContext, url: String) -> bool {
    dcl::js::restricted_actions::op_external_url(state.rc(), url).await
}

#[wasm_bindgen]
pub fn op_emote(op_state: &WorkerContext, emote: String) {
    dcl::js::restricted_actions::op_emote(&mut *op_state.state.borrow_mut(), emote);
}

#[wasm_bindgen]
pub async fn op_scene_emote(
    op_state: &WorkerContext,
    emote: String,
    looping: bool,
) -> Result<(), WasmError> {
    dcl::js::restricted_actions::op_scene_emote(op_state.rc(), emote, looping).await.map_err(|e| WasmError::from(e))
}

#[wasm_bindgen]
pub async fn op_open_nft_dialog(
    op_state: &WorkerContext,
    urn: String,
) -> Result<(), WasmError> {
    dcl::js::restricted_actions::op_open_nft_dialog(op_state.rc(), urn).await.map_err(|e| WasmError::from(e))
}

#[wasm_bindgen]
pub async fn op_set_ui_focus(
    op_state: &WorkerContext,
    element_id: String,
) -> Result<(), WasmError> {
    dcl::js::restricted_actions::op_set_ui_focus(op_state.rc(), element_id).await.map_err(|e| WasmError::from(e))
}
