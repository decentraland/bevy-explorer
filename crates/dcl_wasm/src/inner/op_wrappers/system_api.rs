use std::{cell::RefCell, rc::Rc};

use crate::{serde_parse, serde_result, WasmError, WorkerContext};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub async fn op_check_for_update(state: &mut WorkerContext) -> Result<JsValue, WasmError> {
    serde_result!(dcl::js::system_api::op_check_for_update(Rc::new(RefCell::new(state))).await)
}

#[wasm_bindgen]
pub async fn op_motd(state: &mut WorkerContext) -> Result<String, WasmError> {
    dcl::js::system_api::op_motd(Rc::new(RefCell::new(state)))
        .await
        .map_err(|e| WasmError::from(e))
}

#[wasm_bindgen]
pub fn op_get_current_login(state: &mut WorkerContext) -> Option<String> {
    dcl::js::system_api::op_get_current_login(state)
}

#[wasm_bindgen]
pub async fn op_get_previous_login(state: &mut WorkerContext) -> Result<Option<String>, WasmError> {
    dcl::js::system_api::op_get_previous_login(Rc::new(RefCell::new(state)))
        .await
        .map_err(|e| WasmError::from(e))
}

#[wasm_bindgen]
pub async fn op_login_previous(state: &mut WorkerContext) -> Result<(), WasmError> {
    dcl::js::system_api::op_login_previous(Rc::new(RefCell::new(state)))
        .await
        .map_err(|e| WasmError::from(e))
}

#[wasm_bindgen]
pub async fn op_login_new_code(state: &mut WorkerContext) -> Result<Option<String>, WasmError> {
    dcl::js::system_api::op_login_new_code(Rc::new(RefCell::new(state)))
        .await
        .map_err(|e| WasmError::from(e))
}

#[wasm_bindgen]
pub async fn op_login_new_success(state: &mut WorkerContext) -> Result<(), WasmError> {
    dcl::js::system_api::op_login_new_success(Rc::new(RefCell::new(state)))
        .await
        .map_err(|e| WasmError::from(e))
}

#[wasm_bindgen]
pub fn op_login_guest(state: &mut WorkerContext) {
    dcl::js::system_api::op_login_guest(state)
}

#[wasm_bindgen]
pub fn op_login_cancel(state: &mut WorkerContext) {
    dcl::js::system_api::op_login_cancel(state)
}

#[wasm_bindgen]
pub fn op_logout(state: &mut WorkerContext) {
    dcl::js::system_api::op_logout(state)
}

#[wasm_bindgen]
pub async fn op_settings(state: &mut WorkerContext) -> Result<js_sys::Array, WasmError> {
    dcl::js::system_api::op_settings(Rc::new(RefCell::new(state)))
        .await
        .map(|r| {
            r.into_iter()
                .map(|v| serde_wasm_bindgen::to_value(&v).unwrap())
                .collect()
        })
        .map_err(|e| WasmError::from(e))
}

#[wasm_bindgen]
pub async fn op_set_setting(
    state: &mut WorkerContext,
    name: String,
    val: f32,
) -> Result<(), WasmError> {
    dcl::js::system_api::op_set_setting(Rc::new(RefCell::new(state)), name, val)
        .await
        .map_err(|e| WasmError::from(e))
}

#[wasm_bindgen]
pub async fn op_kernel_fetch_headers(
    state: &mut WorkerContext,
    uri: String,
    method: Option<String>,
    meta: Option<String>,
) -> Result<js_sys::Array, WasmError> {
    dcl::js::system_api::op_kernel_fetch_headers(Rc::new(RefCell::new(state)), uri, method, meta)
        .await
        .map(|r| {
            r.into_iter()
                .map(|v| serde_wasm_bindgen::to_value(&v).unwrap())
                .collect()
        })
        .map_err(|e| WasmError::from(e))
}

#[wasm_bindgen]
pub async fn op_set_avatar(
    state: &mut WorkerContext,
    base: JsValue,
    equip: JsValue,
    has_claimed_name: Option<bool>,
) -> Result<u32, WasmError> {
    serde_parse!(base);
    serde_parse!(equip);
    dcl::js::system_api::op_set_avatar(Rc::new(RefCell::new(state)), base, equip, has_claimed_name)
        .await
        .map_err(|e| WasmError::from(e))
}

#[wasm_bindgen]
pub async fn op_native_input(state: &mut WorkerContext) -> String {
    dcl::js::system_api::op_native_input(Rc::new(RefCell::new(state))).await
}

#[wasm_bindgen]
pub async fn op_get_bindings(state: &mut WorkerContext) -> Result<JsValue, WasmError> {
    serde_result!(dcl::js::system_api::op_get_bindings(Rc::new(RefCell::new(state))).await)
}

#[wasm_bindgen]
pub async fn op_set_bindings(
    state: &mut WorkerContext,
    bindings: JsValue,
) -> Result<(), WasmError> {
    serde_parse!(bindings);
    dcl::js::system_api::op_set_bindings(Rc::new(RefCell::new(state)), bindings)
        .await
        .map_err(|e| WasmError::from(e))
}

#[wasm_bindgen]
pub async fn op_console_command(
    state: &mut WorkerContext,
    cmd: String,
    args: Vec<String>,
) -> Result<String, WasmError> {
    dcl::js::system_api::op_console_command(Rc::new(RefCell::new(state)), cmd, args)
        .await
        .map_err(|e| WasmError::from(e))
}

#[wasm_bindgen]
pub async fn op_live_scene_info(state: &mut WorkerContext) -> Result<js_sys::Array, WasmError> {
    dcl::js::system_api::op_live_scene_info(Rc::new(RefCell::new(state)))
        .await
        .map(|r| {
            r.into_iter()
                .map(|v| serde_wasm_bindgen::to_value(&v).unwrap())
                .collect()
        })
        .map_err(|e| WasmError::from(e))
}

#[wasm_bindgen]
pub async fn op_get_home_scene(state: &mut WorkerContext) -> Result<JsValue, WasmError> {
    serde_result!(dcl::js::system_api::op_get_home_scene(Rc::new(RefCell::new(state))).await)
}

#[wasm_bindgen]
pub fn op_set_home_scene(state: &mut WorkerContext, realm: String, parcel: JsValue) {
    serde_parse!(parcel);
    dcl::js::system_api::op_set_home_scene(Rc::new(RefCell::new(state)), realm, parcel);
}

#[wasm_bindgen]
pub async fn op_get_realm_provider(state: &mut WorkerContext) -> Result<JsValue, WasmError> {
    serde_result!(dcl::js::system_api::op_get_realm_provider(Rc::new(RefCell::new(state))).await)
}

#[wasm_bindgen]
pub async fn op_get_system_action_stream(state: &mut WorkerContext) -> u32 {
    dcl::js::system_api::op_get_system_action_stream(Rc::new(RefCell::new(state))).await
}

#[wasm_bindgen]
pub async fn op_read_system_action_stream(
    state: &mut WorkerContext,
    rid: u32,
) -> Result<JsValue, WasmError> {
    serde_result!(
        dcl::js::system_api::op_read_system_action_stream(Rc::new(RefCell::new(state)), rid).await
    )
}

#[wasm_bindgen]
pub async fn op_get_chat_stream(state: &mut WorkerContext) -> u32 {
    dcl::js::system_api::op_get_chat_stream(Rc::new(RefCell::new(state))).await
}

#[wasm_bindgen]
pub async fn op_read_chat_stream(
    state: &mut WorkerContext,
    rid: u32,
) -> Result<JsValue, WasmError> {
    serde_result!(dcl::js::system_api::op_read_chat_stream(Rc::new(RefCell::new(state)), rid).await)
}

#[wasm_bindgen]
pub fn op_send_chat(state: &mut WorkerContext, message: String, channel: String) {
    dcl::js::system_api::op_send_chat(Rc::new(RefCell::new(state)), message, channel)
}
