use deno_core::{anyhow, error::AnyError, op2, OpDecl, OpState};
use std::{cell::RefCell, rc::Rc};

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![
        op_move_player_to(),
        op_teleport_to(),
        op_change_realm(),
        op_external_url(),
        op_emote(),
        op_scene_emote(),
        op_open_nft_dialog(),
        op_set_ui_focus(),
    ]
}

#[op2(fast)]
#[allow(clippy::too_many_arguments)]
fn op_move_player_to(
    op_state: &mut OpState,
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
async fn op_set_ui_focus(
    op_state: Rc<RefCell<OpState>>,
    #[string] element_id: String,
) -> Result<(), AnyError> {
    dcl::js::restricted_actions::op_set_ui_focus(op_state, element_id).await
}
