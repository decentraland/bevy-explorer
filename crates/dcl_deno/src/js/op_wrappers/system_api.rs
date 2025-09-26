use common::{
    inputs::SystemActionEvent,
    structs::{PermissionType, PermissionUsed, PermissionValue},
};
use dcl::js::system_api::{JsBindingsData, PermissionTypeDetail};
use dcl_component::proto_components::{
    common::Vector2,
    sdk::components::{PbAvatarBase, PbAvatarEquippedData},
};
use deno_core::{anyhow, error::AnyError, op2, OpDecl, OpState};
use std::{cell::RefCell, rc::Rc};
use system_bridge::{
    settings::SettingInfo, ChatMessage, HomeScene, LiveSceneInfo, PermanentPermissionItem,
    PermissionRequest,
};

// list of op declarations
pub fn ops(super_user: bool) -> Vec<OpDecl> {
    if super_user {
        vec![
            op_check_for_update(),
            op_motd(),
            op_get_current_login(),
            op_get_previous_login(),
            op_login_previous(),
            op_login_new_code(),
            op_login_new_success(),
            op_login_cancel(),
            op_login_guest(),
            op_logout(),
            op_settings(),
            op_set_setting(),
            op_kernel_fetch_headers(),
            op_set_avatar(),
            op_native_input(),
            op_get_bindings(),
            op_set_bindings(),
            op_console_command(),
            op_live_scene_info(),
            op_get_home_scene(),
            op_set_home_scene(),
            op_get_system_action_stream(),
            op_read_system_action_stream(),
            op_get_chat_stream(),
            op_read_chat_stream(),
            op_send_chat(),
            op_get_profile_extras(),
            op_quit(),
            op_get_permission_request_stream(),
            op_read_permission_request_stream(),
            op_get_permission_used_stream(),
            op_read_permission_used_stream(),
            op_set_single_permission(),
            op_set_permanent_permission(),
            op_get_permanent_permissions(),
            op_get_permission_types(),
        ]
    } else {
        Vec::default()
    }
}

#[op2(async)]
#[serde]
async fn op_check_for_update(state: Rc<RefCell<OpState>>) -> Result<(String, String), AnyError> {
    dcl::js::system_api::op_check_for_update(state).await
}

#[op2(async)]
#[string]
async fn op_motd(state: Rc<RefCell<OpState>>) -> Result<String, AnyError> {
    dcl::js::system_api::op_motd(state).await
}

#[op2]
#[string]
fn op_get_current_login(state: &mut OpState) -> Option<String> {
    dcl::js::system_api::op_get_current_login(state)
}

#[op2(async)]
#[string]
async fn op_get_previous_login(state: Rc<RefCell<OpState>>) -> Result<Option<String>, AnyError> {
    dcl::js::system_api::op_get_previous_login(state).await
}

#[op2(async)]
#[serde]
async fn op_login_previous(state: Rc<RefCell<OpState>>) -> Result<(), AnyError> {
    dcl::js::system_api::op_login_previous(state).await
}

#[op2(async)]
#[string]
async fn op_login_new_code(state: Rc<RefCell<OpState>>) -> Result<Option<String>, AnyError> {
    dcl::js::system_api::op_login_new_code(state).await
}

#[op2(async)]
#[string]
async fn op_login_new_success(state: Rc<RefCell<OpState>>) -> Result<(), AnyError> {
    dcl::js::system_api::op_login_new_success(state).await
}

#[op2(fast)]
fn op_login_guest(state: &mut OpState) {
    dcl::js::system_api::op_login_guest(state)
}

#[op2(fast)]
fn op_login_cancel(state: &mut OpState) {
    dcl::js::system_api::op_login_cancel(state)
}

#[op2(fast)]
fn op_logout(state: &mut OpState) {
    dcl::js::system_api::op_logout(state)
}

#[op2(async)]
#[serde]
async fn op_settings(state: Rc<RefCell<OpState>>) -> Result<Vec<SettingInfo>, AnyError> {
    dcl::js::system_api::op_settings(state).await
}

#[op2(async)]
#[serde]
async fn op_set_setting(
    state: Rc<RefCell<OpState>>,
    #[string] name: String,
    val: f32,
) -> Result<(), AnyError> {
    dcl::js::system_api::op_set_setting(state, name, val).await
}

#[op2(async)]
#[serde]
pub async fn op_kernel_fetch_headers(
    state: Rc<RefCell<OpState>>,
    #[string] uri: String,
    #[string] method: Option<String>,
    #[string] meta: Option<String>,
) -> Result<Vec<(String, String)>, AnyError> {
    dcl::js::system_api::op_kernel_fetch_headers(state, uri, method, meta).await
}

#[op2(async)]
pub async fn op_set_avatar(
    state: Rc<RefCell<OpState>>,
    #[serde] base: Option<PbAvatarBase>,
    #[serde] equip: Option<PbAvatarEquippedData>,
    has_claimed_name: Option<bool>,
    #[serde] profile_extras: Option<std::collections::HashMap<String, serde_json::Value>>,
) -> Result<u32, anyhow::Error> {
    dcl::js::system_api::op_set_avatar(state, base, equip, has_claimed_name, profile_extras).await
}

#[op2(async)]
#[string]
pub async fn op_native_input(state: Rc<RefCell<OpState>>) -> String {
    dcl::js::system_api::op_native_input(state).await
}

#[op2(async)]
#[serde]
pub async fn op_get_bindings(state: Rc<RefCell<OpState>>) -> Result<JsBindingsData, anyhow::Error> {
    dcl::js::system_api::op_get_bindings(state).await
}

#[op2(async)]
#[serde]
pub async fn op_set_bindings(
    state: Rc<RefCell<OpState>>,
    #[serde] bindings: JsBindingsData,
) -> Result<(), anyhow::Error> {
    dcl::js::system_api::op_set_bindings(state, bindings).await
}

#[op2(async)]
#[string]
pub async fn op_console_command(
    state: Rc<RefCell<OpState>>,
    #[string] cmd: String,
    #[serde] args: Vec<String>,
) -> Result<String, anyhow::Error> {
    dcl::js::system_api::op_console_command(state, cmd, args).await
}

#[op2(async)]
#[serde]
pub async fn op_live_scene_info(
    state: Rc<RefCell<OpState>>,
) -> Result<Vec<LiveSceneInfo>, anyhow::Error> {
    dcl::js::system_api::op_live_scene_info(state).await
}

#[op2(async)]
#[serde]
pub async fn op_get_home_scene(state: Rc<RefCell<OpState>>) -> Result<HomeScene, anyhow::Error> {
    dcl::js::system_api::op_get_home_scene(state).await
}

#[op2]
pub fn op_set_home_scene(
    state: Rc<RefCell<OpState>>,
    #[string] realm: String,
    #[serde] parcel: Vector2,
) {
    dcl::js::system_api::op_set_home_scene(state, realm, parcel);
}

#[op2(async)]
pub async fn op_get_system_action_stream(state: Rc<RefCell<OpState>>) -> u32 {
    dcl::js::system_api::op_get_system_action_stream(state).await
}

#[op2(async)]
#[serde]
pub async fn op_read_system_action_stream(
    state: Rc<RefCell<OpState>>,
    rid: u32,
) -> Result<Option<SystemActionEvent>, deno_core::anyhow::Error> {
    dcl::js::system_api::op_read_system_action_stream(state, rid).await
}

#[op2(async)]
pub async fn op_get_chat_stream(state: Rc<RefCell<OpState>>) -> u32 {
    dcl::js::system_api::op_get_chat_stream(state).await
}

#[op2(async)]
#[serde]
pub async fn op_read_chat_stream(
    state: Rc<RefCell<OpState>>,
    rid: u32,
) -> Result<Option<ChatMessage>, deno_core::anyhow::Error> {
    dcl::js::system_api::op_read_chat_stream(state, rid).await
}

#[op2(fast)]
pub fn op_send_chat(
    state: Rc<RefCell<OpState>>,
    #[string] message: String,
    #[string] channel: String,
) {
    dcl::js::system_api::op_send_chat(state, message, channel)
}

#[op2(async)]
#[serde]
pub async fn op_get_profile_extras(
    state: Rc<RefCell<OpState>>,
) -> Result<std::collections::HashMap<String, serde_json::Value>, deno_core::anyhow::Error> {
    dcl::js::system_api::op_get_profile_extras(state).await
}

#[op2(fast)]
pub fn op_quit(state: Rc<RefCell<OpState>>) {
    dcl::js::system_api::op_quit(state);
}

#[op2(async)]
pub async fn op_get_permission_request_stream(state: Rc<RefCell<OpState>>) -> u32 {
    dcl::js::system_api::op_get_permission_request_stream(state).await
}

#[op2(async)]
#[serde]
pub async fn op_read_permission_request_stream(
    state: Rc<RefCell<OpState>>,
    rid: u32,
) -> Result<Option<PermissionRequest>, deno_core::anyhow::Error> {
    dcl::js::system_api::op_read_permission_request_stream(state, rid).await
}

#[op2(async)]
pub async fn op_get_permission_used_stream(state: Rc<RefCell<OpState>>) -> u32 {
    dcl::js::system_api::op_get_permission_used_stream(state).await
}

#[op2(async)]
#[serde]
pub async fn op_read_permission_used_stream(
    state: Rc<RefCell<OpState>>,
    rid: u32,
) -> Result<Option<PermissionUsed>, deno_core::anyhow::Error> {
    dcl::js::system_api::op_read_permission_used_stream(state, rid).await
}

#[op2(fast)]
pub fn op_set_single_permission(state: Rc<RefCell<OpState>>, #[bigint] id: usize, allow: bool) {
    dcl::js::system_api::op_set_single_permission(state, id, allow);
}

#[op2]
#[serde]
pub fn op_set_permanent_permission(
    state: Rc<RefCell<OpState>>,
    #[string] level: &str,
    #[string] value: Option<String>,
    #[serde] permission_type: PermissionType,
    #[serde] allow: Option<PermissionValue>,
) -> Result<(), anyhow::Error> {
    dcl::js::system_api::op_set_permanent_permission(state, level, value, permission_type, allow)
}

#[op2(async)]
#[serde]
pub async fn op_get_permanent_permissions(
    state: Rc<RefCell<OpState>>,
    #[string] level: String,
    #[string] value: Option<String>,
) -> Result<Vec<PermanentPermissionItem>, anyhow::Error> {
    dcl::js::system_api::op_get_permanent_permissions(state, &level, value).await
}

#[op2]
#[serde]
pub fn op_get_permission_types() -> Vec<PermissionTypeDetail> {
    dcl::js::system_api::op_get_permission_types()
}
