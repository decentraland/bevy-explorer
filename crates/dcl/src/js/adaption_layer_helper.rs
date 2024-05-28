use bevy::math::Vec2;
use common::rpc::RpcCall;
use deno_core::{
    anyhow::{self, anyhow},
    op2, OpDecl, OpState,
};
use serde::Serialize;
use std::{cell::RefCell, rc::Rc};

use crate::{interface::crdt_context::CrdtContext, RpcCalls};

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![op_get_texture_size()]
}

#[derive(Serialize)]
struct TextureSize {
    width: f32,
    height: f32,
}

#[op2(async)]
#[serde]
async fn op_get_texture_size(state: Rc<RefCell<OpState>>, #[string] src: String) -> TextureSize {
    let (sx, rx) = tokio::sync::oneshot::channel::<Result<Vec2, String>>();
    let scene = state.borrow().borrow::<CrdtContext>().scene_id.0;

    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::GetTextureSize {
            scene,
            src,
            response: sx.into(),
        });

    let Ok(result) = rx.await.map_err(|e| anyhow::anyhow!(e)) else {
        return TextureSize {
            width: 1.0,
            height: 1.0,
        };
    };

    result
        .map_err(|e| anyhow!(e))
        .map(|v| TextureSize {
            width: v.x,
            height: v.y,
        })
        .unwrap_or(TextureSize {
            width: 1.0,
            height: 1.0,
        })
}
