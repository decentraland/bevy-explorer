use std::{cell::RefCell, rc::Rc};

use anyhow::anyhow;
use common::rpc::{RpcCall, RpcResultSender};
use serde::Serialize;

use crate::{interface::crdt_context::CrdtContext, RpcCalls};

use super::State;

#[derive(Serialize)]
pub struct TextureSize {
    width: f32,
    height: f32,
}

pub async fn op_get_texture_size(state: Rc<RefCell<impl State>>, src: String) -> TextureSize {
    let (sx, rx) = RpcResultSender::channel();
    let scene = state.borrow().borrow::<CrdtContext>().scene_id.0;

    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::GetTextureSize {
            scene,
            src,
            response: sx,
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
