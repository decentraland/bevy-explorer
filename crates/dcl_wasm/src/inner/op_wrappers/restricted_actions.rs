use crate::{serde_result, WasmError, WorkerContext};
use dcl_component::proto_components::common::Vector3 as DclVector3;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub async fn op_move_player_to(
    op_state: &WorkerContext,
    position: JsValue,
    camera_target: JsValue,
    avatar_target: JsValue,
    duration: Option<f32>,
) -> bool {
    let position: DclVector3 = serde_wasm_bindgen::from_value(position).unwrap_or_default();
    let camera_target: Option<DclVector3> = serde_wasm_bindgen::from_value(camera_target).ok();
    let avatar_target: Option<DclVector3> = serde_wasm_bindgen::from_value(avatar_target).ok();
    dcl::js::restricted_actions::op_move_player_to(
        op_state.rc(),
        position,
        camera_target,
        avatar_target,
        duration,
    )
    .await
}

#[wasm_bindgen]
pub async fn op_walk_player_to(
    op_state: &WorkerContext,
    position: JsValue,
    stop_threshold: f32,
    timeout: Option<f32>,
) -> bool {
    let position: DclVector3 = serde_wasm_bindgen::from_value(position).unwrap_or_default();
    dcl::js::restricted_actions::op_walk_player_to(op_state.rc(), position, stop_threshold, timeout)
        .await
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
    dcl::js::restricted_actions::op_scene_emote(op_state.rc(), emote, looping)
        .await
        .map_err(WasmError::from)
}

#[wasm_bindgen]
pub async fn op_open_nft_dialog(op_state: &WorkerContext, urn: String) -> Result<(), WasmError> {
    dcl::js::restricted_actions::op_open_nft_dialog(op_state.rc(), urn)
        .await
        .map_err(WasmError::from)
}

#[wasm_bindgen]
pub async fn op_ui_focus(
    op_state: &WorkerContext,
    apply: bool,
    element_id: Option<String>,
) -> Result<JsValue, WasmError> {
    serde_result!(dcl::js::restricted_actions::op_ui_focus(op_state.rc(), apply, element_id).await)
}

#[wasm_bindgen]
pub async fn op_copy_to_clipboard(op_state: &WorkerContext, text: String) -> Result<(), WasmError> {
    dcl::js::restricted_actions::op_copy_to_clipboard(op_state.rc(), text)
        .await
        .map_err(WasmError::from)
}
