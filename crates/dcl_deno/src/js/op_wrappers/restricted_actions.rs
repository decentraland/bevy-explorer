use dcl::js::restricted_actions::UiFocusResult;
use dcl_component::proto_components::common::Vector3 as DclVector3;
use deno_core::{anyhow, error::AnyError, op2, OpDecl, OpState};
use std::{cell::RefCell, rc::Rc};

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![
        op_move_player_to(),
        op_walk_player_to(),
        op_teleport_to(),
        op_change_realm(),
        op_external_url(),
        op_emote(),
        op_scene_emote(),
        op_open_nft_dialog(),
        op_ui_focus(),
        op_copy_to_clipboard(),
    ]
}

#[op2(async)]
async fn op_move_player_to(
    state: Rc<RefCell<OpState>>,
    #[serde] position: DclVector3,
    #[serde] camera_target: Option<DclVector3>,
    #[serde] avatar_target: Option<DclVector3>,
    duration: Option<f32>,
) -> bool {
    dcl::js::restricted_actions::op_move_player_to(
        state,
        position,
        camera_target,
        avatar_target,
        duration,
    )
    .await
}

#[op2(async)]
async fn op_walk_player_to(
    state: Rc<RefCell<OpState>>,
    #[serde] position: DclVector3,
    stop_threshold: f32,
    timeout: Option<f32>,
) -> bool {
    dcl::js::restricted_actions::op_walk_player_to(state, position, stop_threshold, timeout).await
}

#[op2(async)]
async fn op_teleport_to(state: Rc<RefCell<OpState>>, position_x: i32, position_y: i32) -> bool {
    dcl::js::restricted_actions::op_teleport_to(state, position_x, position_y).await
}

#[op2(async)]
async fn op_change_realm(
    state: Rc<RefCell<OpState>>,
    #[string] realm: String,
    #[string] message: Option<String>,
) -> bool {
    dcl::js::restricted_actions::op_change_realm(state, realm, message).await
}

#[op2(async)]
async fn op_external_url(state: Rc<RefCell<OpState>>, #[string] url: String) -> bool {
    dcl::js::restricted_actions::op_external_url(state, url).await
}

#[op2(fast)]
fn op_emote(op_state: &mut OpState, #[string] emote: String) {
    dcl::js::restricted_actions::op_emote(op_state, emote);
}

#[op2(async)]
async fn op_scene_emote(
    op_state: Rc<RefCell<OpState>>,
    #[string] emote: String,
    looping: bool,
) -> Result<(), anyhow::Error> {
    dcl::js::restricted_actions::op_scene_emote(op_state, emote, looping).await
}

#[op2(async)]
async fn op_open_nft_dialog(
    op_state: Rc<RefCell<OpState>>,
    #[string] urn: String,
) -> Result<(), AnyError> {
    dcl::js::restricted_actions::op_open_nft_dialog(op_state, urn).await
}

#[op2(async)]
#[serde]
async fn op_ui_focus(
    op_state: Rc<RefCell<OpState>>,
    apply: bool,
    #[string] element_id: Option<String>,
) -> Result<UiFocusResult, AnyError> {
    dcl::js::restricted_actions::op_ui_focus(op_state, apply, element_id).await
}

#[op2(async)]
async fn op_copy_to_clipboard(
    op_state: Rc<RefCell<OpState>>,
    #[string] text: String,
) -> Result<(), AnyError> {
    dcl::js::restricted_actions::op_copy_to_clipboard(op_state, text).await
}
