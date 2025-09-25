use crate::{serde_parse, serde_result, WasmError, WorkerContext};
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub async fn op_check_for_update(state: &WorkerContext) -> Result<JsValue, WasmError> {
    serde_result!(dcl::js::system_api::op_check_for_update(state.rc()).await)
}

#[wasm_bindgen]
pub async fn op_motd(state: &WorkerContext) -> Result<String, WasmError> {
    dcl::js::system_api::op_motd(state.rc())
        .await
        .map_err(WasmError::from)
}

#[wasm_bindgen]
pub fn op_get_current_login(state: &WorkerContext) -> Option<String> {
    dcl::js::system_api::op_get_current_login(&mut *state.state.borrow_mut())
}

#[wasm_bindgen]
pub async fn op_get_previous_login(state: &WorkerContext) -> Result<Option<String>, WasmError> {
    dcl::js::system_api::op_get_previous_login(state.rc())
        .await
        .map_err(WasmError::from)
}

#[wasm_bindgen]
pub async fn op_login_previous(state: &WorkerContext) -> Result<(), WasmError> {
    dcl::js::system_api::op_login_previous(state.rc())
        .await
        .map_err(WasmError::from)
}

#[wasm_bindgen]
pub async fn op_login_new_code(state: &WorkerContext) -> Result<Option<String>, WasmError> {
    dcl::js::system_api::op_login_new_code(state.rc())
        .await
        .map_err(WasmError::from)
}

#[wasm_bindgen]
pub async fn op_login_new_success(state: &WorkerContext) -> Result<(), WasmError> {
    dcl::js::system_api::op_login_new_success(state.rc())
        .await
        .map_err(WasmError::from)
}

#[wasm_bindgen]
pub fn op_login_guest(state: &WorkerContext) {
    dcl::js::system_api::op_login_guest(&mut *state.state.borrow_mut())
}

#[wasm_bindgen]
pub fn op_login_cancel(state: &WorkerContext) {
    dcl::js::system_api::op_login_cancel(&mut *state.state.borrow_mut())
}

#[wasm_bindgen]
pub fn op_logout(state: &WorkerContext) {
    dcl::js::system_api::op_logout(&mut *state.state.borrow_mut())
}

#[wasm_bindgen]
pub async fn op_settings(state: &WorkerContext) -> Result<js_sys::Array, WasmError> {
    dcl::js::system_api::op_settings(state.rc())
        .await
        .map(|r| {
            r.into_iter()
                .map(|v| serde_wasm_bindgen::to_value(&v).unwrap())
                .collect()
        })
        .map_err(WasmError::from)
}

#[wasm_bindgen]
pub async fn op_set_setting(
    state: &WorkerContext,
    name: String,
    val: f32,
) -> Result<(), WasmError> {
    dcl::js::system_api::op_set_setting(state.rc(), name, val)
        .await
        .map_err(WasmError::from)
}

#[wasm_bindgen]
pub async fn op_kernel_fetch_headers(
    state: &WorkerContext,
    uri: String,
    method: Option<String>,
    meta: Option<String>,
) -> Result<js_sys::Array, WasmError> {
    dcl::js::system_api::op_kernel_fetch_headers(state.rc(), uri, method, meta)
        .await
        .map(|r| {
            r.into_iter()
                .map(|v| serde_wasm_bindgen::to_value(&v).unwrap())
                .collect()
        })
        .map_err(WasmError::from)
}

#[wasm_bindgen]
pub async fn op_set_avatar(
    state: &WorkerContext,
    base: JsValue,
    equip: JsValue,
    has_claimed_name: Option<bool>,
    profile_extras: JsValue,
) -> Result<u32, WasmError> {
    serde_parse!(base);
    serde_parse!(equip);
    serde_parse!(profile_extras);
    dcl::js::system_api::op_set_avatar(state.rc(), base, equip, has_claimed_name, profile_extras)
        .await
        .map_err(WasmError::from)
}

#[wasm_bindgen]
pub async fn op_native_input(state: &WorkerContext) -> String {
    dcl::js::system_api::op_native_input(state.rc()).await
}

#[wasm_bindgen]
pub async fn op_get_bindings(state: &WorkerContext) -> Result<JsValue, WasmError> {
    serde_result!(dcl::js::system_api::op_get_bindings(state.rc()).await)
}

#[wasm_bindgen]
pub async fn op_set_bindings(state: &WorkerContext, bindings: JsValue) -> Result<(), WasmError> {
    serde_parse!(bindings);
    dcl::js::system_api::op_set_bindings(state.rc(), bindings)
        .await
        .map_err(WasmError::from)
}

#[wasm_bindgen]
pub async fn op_console_command(
    state: &WorkerContext,
    cmd: String,
    args: Vec<String>,
) -> Result<String, WasmError> {
    dcl::js::system_api::op_console_command(state.rc(), cmd, args)
        .await
        .map_err(WasmError::from)
}

#[wasm_bindgen]
pub async fn op_live_scene_info(state: &WorkerContext) -> Result<js_sys::Array, WasmError> {
    dcl::js::system_api::op_live_scene_info(state.rc())
        .await
        .map(|r| {
            r.into_iter()
                .map(|v| serde_wasm_bindgen::to_value(&v).unwrap())
                .collect()
        })
        .map_err(WasmError::from)
}

#[wasm_bindgen]
pub async fn op_get_home_scene(state: &WorkerContext) -> Result<JsValue, WasmError> {
    serde_result!(dcl::js::system_api::op_get_home_scene(state.rc()).await)
}

#[wasm_bindgen]
pub fn op_set_home_scene(state: &WorkerContext, realm: String, parcel: JsValue) {
    serde_parse!(parcel);
    dcl::js::system_api::op_set_home_scene(state.rc(), realm, parcel);
}

#[wasm_bindgen]
pub async fn op_get_system_action_stream(state: &WorkerContext) -> u32 {
    dcl::js::system_api::op_get_system_action_stream(state.rc()).await
}

#[wasm_bindgen]
pub async fn op_read_system_action_stream(
    state: &WorkerContext,
    rid: u32,
) -> Result<JsValue, WasmError> {
    serde_result!(dcl::js::system_api::op_read_system_action_stream(state.rc(), rid).await)
}

#[wasm_bindgen]
pub async fn op_get_chat_stream(state: &WorkerContext) -> u32 {
    dcl::js::system_api::op_get_chat_stream(state.rc()).await
}

#[wasm_bindgen]
pub async fn op_read_chat_stream(state: &WorkerContext, rid: u32) -> Result<JsValue, WasmError> {
    serde_result!(dcl::js::system_api::op_read_chat_stream(state.rc(), rid).await)
}

#[wasm_bindgen]
pub fn op_send_chat(state: &WorkerContext, message: String, channel: String) {
    dcl::js::system_api::op_send_chat(state.rc(), message, channel)
}

#[wasm_bindgen]
pub async fn op_get_profile_extras(state: &WorkerContext) -> Result<JsValue, WasmError> {
    let extras = dcl::js::system_api::op_get_profile_extras(state.rc()).await;
    // use a specific serializer to convert to object here, as wasm_bindgen's conversion otherwise produces a Map
    extras
        .map(|v| {
            v.serialize(&serde_wasm_bindgen::Serializer::json_compatible())
                .unwrap()
        })
        .map_err(WasmError::from)
}

#[wasm_bindgen]
pub fn op_quit(state: &WorkerContext) {
    dcl::js::system_api::op_quit(state.rc());
}

#[wasm_bindgen]
pub async fn op_get_permission_request_stream(state: &WorkerContext) -> u32 {
    dcl::js::system_api::op_get_permission_request_stream(state.rc()).await
}

#[wasm_bindgen]
pub async fn op_read_permission_request_stream(
    state: &WorkerContext,
    rid: u32,
) -> Result<JsValue, WasmError> {
    serde_result!(dcl::js::system_api::op_read_permission_request_stream(state.rc(), rid).await)
}

#[wasm_bindgen]
pub async fn op_get_permission_used_stream(state: &WorkerContext) -> u32 {
    dcl::js::system_api::op_get_permission_used_stream(state.rc()).await
}

#[wasm_bindgen]
pub async fn op_read_permission_used_stream(
    state: &WorkerContext,
    rid: u32,
) -> Result<JsValue, WasmError> {
    serde_result!(dcl::js::system_api::op_read_permission_used_stream(state.rc(), rid).await)
}

#[wasm_bindgen]
pub fn op_set_single_permission(state: &WorkerContext, id: usize, allow: bool) {
    dcl::js::system_api::op_set_single_permission(state.rc(), id, allow);
}

#[wasm_bindgen]
pub fn op_set_permanent_permission(
    state: &WorkerContext,
    level: &str,
    value: Option<String>,
    permission_type: JsValue,
    allow: JsValue,
) -> Result<(), WasmError> {
    serde_parse!(permission_type);
    serde_parse!(allow);
    dcl::js::system_api::op_set_permanent_permission(
        state.rc(),
        level,
        value,
        permission_type,
        allow,
    )
    .map_err(WasmError::from)
}

#[wasm_bindgen]
pub async fn op_get_permanent_permissions(
    state: &WorkerContext,
    level: &str,
    value: Option<String>,
) -> Result<js_sys::Array, WasmError> {
    dcl::js::system_api::op_get_permanent_permissions(state.rc(), level, value)
        .await
        .map(|r| {
            r.into_iter()
                .map(|v| serde_wasm_bindgen::to_value(&v).unwrap())
                .collect()
        })
        .map_err(WasmError::from)
}

#[wasm_bindgen]
pub fn op_get_permission_types(_: &WorkerContext) -> js_sys::Array {
    dcl::js::system_api::op_get_permission_types()
        .into_iter()
        .map(|v| serde_wasm_bindgen::to_value(&v).unwrap())
        .collect()
}
