use bevy::{log::debug, math::IVec2};
use common::rpc::RpcCall;
use deno_core::{
    anyhow::{self, anyhow},
    error::AnyError,
    op2, Op, OpDecl, OpState,
};
use std::{cell::RefCell, rc::Rc};

use crate::{
    interface::{crdt_context::CrdtContext, CrdtType},
    js::RendererStore,
    CrdtStore,
};
use dcl_component::{
    proto_components::sdk::components::PbAvatarEmoteCommand,
    transform_and_parent::{DclTransformAndParent, DclTranslation},
    DclReader, DclWriter, SceneComponentId, SceneEntityId,
};

use super::{runtime::scene_information, RpcCalls};

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![
        op_move_player_to::DECL,
        op_teleport_to::DECL,
        op_change_realm::DECL,
        op_external_url::DECL,
        op_emote::DECL,
        op_scene_emote::DECL,
        op_open_nft_dialog::DECL,
    ]
}

#[op2]
fn op_move_player_to(
    op_state: &mut OpState,
    absolute: bool,
    #[serde] position: (f32, f32, f32),
    #[serde] maybe_camera: Option<(f32, f32, f32)>,
) {
    let position = [position.0, position.1, position.2];
    let maybe_camera = maybe_camera.map(|cam| [cam.0, cam.1, cam.2]);

    debug!("move player to {:?}", position);
    let scene = op_state.borrow::<CrdtContext>().scene_id.0;

    // get current
    let inbound = &op_state.borrow::<RendererStore>().0;
    let get_transform = |id: SceneEntityId| -> DclTransformAndParent {
        DclReader::new(
            &inbound
                .lww
                .get(&SceneComponentId::TRANSFORM)
                .unwrap()
                .last_write
                .get(&id)
                .unwrap()
                .data,
        )
        .read()
        .unwrap()
    };

    let to = if absolute {
        let origin = get_transform(SceneEntityId::WORLD_ORIGIN).translation;
        DclTranslation(position) - origin
    } else {
        DclTranslation(position)
    }
    .to_bevy_translation();

    let looking_at = maybe_camera.map(|camera| {
        if absolute {
            let origin = get_transform(SceneEntityId::WORLD_ORIGIN).translation;
            DclTranslation(camera) - origin
        } else {
            DclTranslation(camera)
        }
        .to_bevy_translation()
    });

    op_state.borrow_mut::<RpcCalls>().push(RpcCall::MovePlayer {
        scene,
        to,
        looking_at,
    });

    if let Some(looking_at) = looking_at {
        op_state.borrow_mut::<RpcCalls>().push(RpcCall::MoveCamera {
            scene,
            to: looking_at,
        });
    }
}

#[op2(async)]
async fn op_teleport_to(state: Rc<RefCell<OpState>>, #[serde] position: (i32, i32)) -> bool {
    let (sx, rx) = tokio::sync::oneshot::channel::<Result<(), String>>();
    let scene = state.borrow().borrow::<CrdtContext>().scene_id.0;
    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::TeleportPlayer {
            scene: Some(scene),
            to: IVec2::new(position.0, position.1),
            response: sx.into(),
        });

    matches!(rx.await, Ok(Ok(_)))
}

#[op2(async)]
async fn op_change_realm(
    state: Rc<RefCell<OpState>>,
    #[string] realm: String,
    #[string] message: Option<String>,
) -> bool {
    let (sx, rx) = tokio::sync::oneshot::channel::<Result<(), String>>();
    let scene = state.borrow().borrow::<CrdtContext>().scene_id.0;
    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::ChangeRealm {
            scene,
            to: realm,
            message,
            response: sx.into(),
        });

    matches!(rx.await, Ok(Ok(_)))
}

#[op2(async)]
async fn op_external_url(state: Rc<RefCell<OpState>>, #[string] url: String) -> bool {
    let (sx, rx) = tokio::sync::oneshot::channel::<Result<(), String>>();
    let scene = state.borrow().borrow::<CrdtContext>().scene_id.0;
    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::ExternalUrl {
            scene,
            url,
            response: sx.into(),
        });

    matches!(rx.await, Ok(Ok(_)))
}

#[op2(fast)]
fn op_emote(op_state: &mut OpState, #[string] emote: String) {
    let emote = PbAvatarEmoteCommand {
        emote_urn: emote,
        r#loop: false,
        timestamp: 0,
    };

    send_emote(op_state, emote);
}

#[op2(async)]
async fn op_scene_emote(
    op_state: Rc<RefCell<OpState>>,
    #[string] emote: String,
    looping: bool,
) -> Result<(), anyhow::Error> {
    let scene_info = scene_information(op_state.clone()).await?;

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
    let emote_urn = format!("urn:decentraland:off-chain:scene-emote:{emote_hash}-{looping}");

    let emote = PbAvatarEmoteCommand {
        emote_urn,
        r#loop: looping,
        timestamp: 0,
    };

    send_emote(&mut op_state.borrow_mut(), emote);
    Ok(())
}

fn send_emote(op_state: &mut OpState, emote: PbAvatarEmoteCommand) {
    //ensure entity
    let context = op_state.borrow_mut::<CrdtContext>();
    context.init(SceneEntityId::PLAYER);

    // write update
    let outbound = op_state.borrow_mut::<CrdtStore>();
    let mut buf = Vec::default();
    DclWriter::new(&mut buf).write(&emote);
    outbound.force_update(
        SceneComponentId::AVATAR_EMOTE_COMMAND,
        CrdtType::GO_ANY,
        SceneEntityId::PLAYER,
        Some(&mut DclReader::new(&buf)),
    );
}

#[op2(async)]
async fn op_open_nft_dialog(
    op_state: Rc<RefCell<OpState>>,
    #[string] urn: String,
) -> Result<(), AnyError> {
    let (sx, rx) = tokio::sync::oneshot::channel::<Result<(), String>>();

    {
        let mut state = op_state.borrow_mut();
        let context = state.borrow::<CrdtContext>();
        let scene = context.scene_id.0;

        state.borrow_mut::<RpcCalls>().push(RpcCall::OpenNftDialog {
            scene,
            urn,
            response: sx.into(),
        });
    }

    rx.await.map_err(|e| anyhow!(e))?.map_err(|e| anyhow!(e))
}
