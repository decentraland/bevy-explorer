use anyhow::anyhow;
use bevy::{
    log::debug,
    math::{IVec2, Vec3},
    transform::components::Transform,
};
use common::rpc::{RpcCall, RpcResultSender, RpcUiFocusAction};
use serde::Serialize;
use std::{cell::RefCell, rc::Rc};

use crate::{interface::crdt_context::CrdtContext, RpcCalls};
use dcl_component::transform_and_parent::DclTranslation;

use super::{runtime::scene_information, State};

#[allow(clippy::too_many_arguments)]
pub fn op_move_player_to(
    op_state: &mut impl State,
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
    let position = [position_x, position_y, position_z];
    let maybe_camera = if camera {
        Some([maybe_camera_x, maybe_camera_y, maybe_camera_z])
    } else {
        None
    };
    let maybe_looking_at = if looking_at {
        Some([maybe_looking_at_x, maybe_looking_at_y, maybe_looking_at_z])
    } else {
        None
    };

    debug!("move player to {position:?}, camera: {maybe_camera:?}, rotate: {maybe_looking_at:?}");
    let scene = op_state.borrow::<CrdtContext>().scene_id.0;

    let to = DclTranslation(position).to_bevy_translation();

    let avatar_target = maybe_looking_at.or(maybe_camera);
    let looking_at = avatar_target.map(|target| DclTranslation(target).to_bevy_translation());

    op_state.borrow_mut::<RpcCalls>().push(RpcCall::MovePlayer {
        scene,
        to,
        looking_at,
    });

    let camera_rotation = maybe_camera.map(|camera| {
        let camera_target = DclTranslation(camera).to_bevy_translation();
        Transform::IDENTITY
            .looking_at(camera_target - to, Vec3::Y)
            .rotation
    });
    if let Some(facing) = camera_rotation {
        op_state
            .borrow_mut::<RpcCalls>()
            .push(RpcCall::MoveCamera { scene, facing });
    }
}

pub async fn op_teleport_to(
    state: Rc<RefCell<impl State>>,
    position_x: i32,
    position_y: i32,
) -> bool {
    debug!("op_teleport_to");
    let (sx, rx) = RpcResultSender::<Result<(), String>>::channel();
    let scene = state.borrow().borrow::<CrdtContext>().scene_id.0;
    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::TeleportPlayer {
            scene: Some(scene),
            to: IVec2::new(position_x, position_y),
            response: sx,
        });

    matches!(rx.await, Ok(Ok(_)))
}

pub async fn op_change_realm(
    state: Rc<RefCell<impl State>>,
    realm: String,
    message: Option<String>,
) -> bool {
    debug!("op_change_realm");
    let (sx, rx) = RpcResultSender::<Result<(), String>>::channel();
    let scene = state.borrow().borrow::<CrdtContext>().scene_id.0;
    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::ChangeRealm {
            scene,
            to: realm,
            message,
            response: sx,
        });

    matches!(rx.await, Ok(Ok(_)))
}

pub async fn op_external_url(state: Rc<RefCell<impl State>>, url: String) -> bool {
    debug!("op_external_url");
    let (sx, rx) = RpcResultSender::<Result<(), String>>::channel();
    let scene = state.borrow().borrow::<CrdtContext>().scene_id.0;
    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::ExternalUrl {
            scene,
            url,
            response: sx,
        });

    matches!(rx.await, Ok(Ok(_)))
}

pub fn op_emote(op_state: &mut impl State, emote: String) {
    debug!("op_emote");
    send_emote(op_state, emote, false);
}

pub async fn op_scene_emote(
    op_state: Rc<RefCell<impl State>>,
    emote: String,
    looping: bool,
) -> Result<(), anyhow::Error> {
    debug!("op_scene_emote");
    let scene_info = scene_information(op_state.clone()).await?;

    let scene_hash = &scene_info.urn;
    let emote = emote.to_lowercase();
    let emote_hash = &scene_info
        .content
        .iter()
        .find(|fe| fe.file == emote)
        .ok_or(anyhow!(
            "emote not found in content map: {} not in {:?}",
            emote,
            scene_info
                .content
                .iter()
                .map(|fe| &fe.file)
                .collect::<Vec<_>>()
        ))?
        .hash;
    let emote_urn =
        format!("urn:decentraland:off-chain:scene-emote:{scene_hash}-{emote_hash}-{looping}");

    send_emote(&mut *op_state.borrow_mut(), emote_urn, looping);
    Ok(())
}

pub fn send_emote(op_state: &mut impl State, urn: String, r#loop: bool) {
    let context = op_state.borrow::<CrdtContext>();
    let scene = context.scene_id.0;

    op_state
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::TriggerEmote { scene, urn, r#loop });
}

pub async fn op_open_nft_dialog(
    op_state: Rc<RefCell<impl State>>,
    urn: String,
) -> Result<(), anyhow::Error> {
    debug!("op_open_nft_dialog");
    let (sx, rx) = RpcResultSender::<Result<(), String>>::channel();

    {
        let mut state = op_state.borrow_mut();
        let context = state.borrow::<CrdtContext>();
        let scene = context.scene_id.0;

        state.borrow_mut::<RpcCalls>().push(RpcCall::OpenNftDialog {
            scene,
            urn,
            response: sx,
        });
    }

    rx.await.map_err(|e| anyhow!(e))?.map_err(|e| anyhow!(e))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiFocusResult {
    element_id: Option<String>,
}

pub async fn op_ui_focus(
    op_state: Rc<RefCell<impl State>>,
    apply: bool,
    element_id: Option<String>,
) -> Result<UiFocusResult, anyhow::Error> {
    debug!("op_ui_focus");
    let (sx, rx) = RpcResultSender::<Result<Option<String>, String>>::channel();

    {
        let mut state = op_state.borrow_mut();
        let context = state.borrow::<CrdtContext>();
        let scene = context.scene_id.0;

        let element_id = element_id.unwrap_or_default();
        let action = match (apply, element_id.is_empty()) {
            (true, true) => RpcUiFocusAction::Defocus,
            (true, false) => RpcUiFocusAction::Focus { element_id },
            (false, _) => RpcUiFocusAction::GetFocus,
        };

        state.borrow_mut::<RpcCalls>().push(RpcCall::UiFocus {
            scene,
            action,
            response: sx,
        });
    }

    rx.await
        .map_err(|e| anyhow!(e))?
        .map(|element_id| UiFocusResult { element_id })
        .map_err(|e| anyhow!(e))
}

pub async fn op_copy_to_clipboard(
    state: Rc<RefCell<impl State>>,
    text: String,
) -> Result<(), anyhow::Error> {
    debug!("op_set_ui_focus");
    let (sx, rx) = RpcResultSender::<Result<(), String>>::channel();

    {
        let mut state = state.borrow_mut();
        let scene = state.borrow::<CrdtContext>().scene_id.0;

        state
            .borrow_mut::<RpcCalls>()
            .push(RpcCall::CopyToClipboard {
                scene,
                text,
                response: sx,
            });
    }

    rx.await.map_err(|e| anyhow!(e))?.map_err(|e| anyhow!(e))
}
