use std::{cell::RefCell, rc::Rc};

use anyhow::anyhow;
use bevy::log::debug;
use common::rpc::{RpcCall, RpcResultSender};

use crate::{js::State, RpcCalls};

pub async fn op_signed_fetch_headers(
    state: Rc<RefCell<impl State>>,
    uri: String,
    method: Option<String>,
) -> Result<Vec<(String, String)>, anyhow::Error> {
    debug!("op_signed_fetch_headers");

    let (sx, rx) = RpcResultSender::channel();

    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::SignRequest {
            method: method.unwrap_or_else(|| String::from("get")),
            uri,
            response: sx,
        })?;

    rx.await?.map_err(|e| anyhow!(e))
}
