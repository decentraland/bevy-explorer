use bevy::prelude::{Mat3, Quat, Vec3};
use common::structs::SceneRpcCall;
use deno_core::{op, Op, OpDecl, OpState};
use std::{cell::RefCell, rc::Rc};

use crate::{interface::CrdtType, js::RendererStore, CrdtStore};
use dcl_component::{
    transform_and_parent::{DclQuat, DclTransformAndParent, DclTranslation},
    DclReader, DclWriter, SceneComponentId, SceneEntityId,
};

use super::RpcCalls;

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![
        op_move_player_to::DECL,
        op_change_realm::DECL,
        op_external_url::DECL,
    ]
}

#[op(v8)]
fn op_move_player_to(
    op_state: Rc<RefCell<OpState>>,
    absolute: bool,
    position: [f32; 3],
    maybe_camera: Option<[f32; 3]>,
) {
    let mut op_state = op_state.borrow_mut();

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

    let mut player_transform = get_transform(SceneEntityId::PLAYER);
    let mut camera_transform = get_transform(SceneEntityId::CAMERA);

    // update
    let look_to = |direction: Vec3| -> DclQuat {
        let back = -direction.try_normalize().unwrap_or(Vec3::Z);
        let right = Vec3::Y
            .cross(back)
            .try_normalize()
            .unwrap_or_else(|| Vec3::Y.any_orthonormal_vector());
        let up = back.cross(right);
        DclQuat::from_bevy_quat(Quat::from_mat3(&Mat3::from_cols(right, up, back)))
    };

    player_transform.translation = if absolute {
        let origin = get_transform(SceneEntityId::WORLD_ORIGIN).translation;
        DclTranslation(position) - origin
    } else {
        DclTranslation(position)
    };

    if let Some(camera) = maybe_camera {
        let target_offset = Vec3 {
            x: camera[0] - position[0],
            y: camera[1] - position[1],
            z: position[2] - camera[2], // flip z
        };
        camera_transform.rotation = look_to(target_offset);
        player_transform.rotation = look_to(target_offset * (Vec3::X + Vec3::Z));
    }

    // write commands
    let mut buf = Vec::default();
    let outbound = op_state.borrow_mut::<CrdtStore>();

    DclWriter::new(&mut buf).write(&player_transform);
    outbound.force_update(
        SceneComponentId::TRANSFORM,
        CrdtType::LWW_ANY,
        SceneEntityId::PLAYER,
        Some(&mut DclReader::new(&buf)),
    );

    if maybe_camera.is_some() {
        DclWriter::new(&mut buf).write(&camera_transform);
        outbound.force_update(
            SceneComponentId::TRANSFORM,
            CrdtType::LWW_ANY,
            SceneEntityId::CAMERA,
            Some(&mut DclReader::new(&buf)),
        );
    }
}

#[op]
async fn op_change_realm(
    state: Rc<RefCell<OpState>>,
    realm: String,
    message: Option<String>,
) -> bool {
    let (sx, rx) = tokio::sync::oneshot::channel::<Result<String, String>>();
    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push((SceneRpcCall::ChangeRealm { to: realm, message }, Some(sx)));

    matches!(rx.await, Ok(Ok(_)))
}

#[op]
async fn op_external_url(state: Rc<RefCell<OpState>>, url: String) -> bool {
    let (sx, rx) = tokio::sync::oneshot::channel::<Result<String, String>>();
    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push((SceneRpcCall::ExternalUrl { url }, Some(sx)));

    matches!(rx.await, Ok(Ok(_)))
}
