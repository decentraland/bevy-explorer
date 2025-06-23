use std::{cell::RefCell, rc::Rc};

use bevy::log::debug;
use common::{rpc::RpcCall, util::AsH160};
use serde::{Deserialize, Serialize};

use crate::{interface::crdt_context::CrdtContext, RpcCalls};

use super::State;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum CommsMessageType {
    String = 1,
    Binary = 2,
}

#[derive(Serialize, Deserialize)]
pub struct MessageBusMessage {
    sender: String,
    data: Vec<u8>,
}

struct BinaryBusReceiver(tokio::sync::mpsc::UnboundedReceiver<(String, Vec<u8>)>);

pub async fn op_comms_send_string(state: Rc<RefCell<impl State>>, message: String) {
    debug!("op_comms_send_string");
    let mut state = state.borrow_mut();
    let scene = state.borrow::<CrdtContext>().scene_id.0;
    let mut data = vec![CommsMessageType::String as u8];
    data.extend(message.into_bytes());
    state
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::SendMessageBus {
            scene,
            data,
            recipient: None,
        });
}

pub async fn op_comms_send_binary_single(
    state: Rc<RefCell<impl State>>,
    message: impl AsRef<[u8]>,
    recipient: Option<String>,
) {
    debug!("op_comms_send_binary_single");
    let mut state = state.borrow_mut();

    let context = state.borrow::<CrdtContext>();
    let scene = context.scene_id.0;
    let mut data = vec![CommsMessageType::Binary as u8];
    data.extend(message.as_ref());

    let recipient = recipient.and_then(|r| r.as_h160());

    state
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::SendMessageBus {
            scene,
            data,
            recipient,
        });
}

pub async fn op_comms_recv_binary(
    state: Rc<RefCell<impl State>>,
) -> Result<Vec<Vec<u8>>, anyhow::Error> {
    debug!("op_comms_recv_binary");
    let mut state = state.borrow_mut();

    let context = state.borrow::<CrdtContext>();
    let hash = context.hash.clone();

    let mut results = Vec::default();

    if !state.has::<BinaryBusReceiver>() {
        let (sx, rx) = tokio::sync::mpsc::unbounded_channel::<(String, Vec<u8>)>();
        state
            .borrow_mut::<RpcCalls>()
            .push(RpcCall::SubscribeBinaryBus { hash, sender: sx });
        state.put(BinaryBusReceiver(rx));
    }

    let rx = state.borrow_mut::<BinaryBusReceiver>();
    while let Ok((sender, data)) = rx.0.try_recv() {
        let sender = sender.into_bytes();
        let mut response = vec![sender.len() as u8];
        response.extend(sender);
        response.extend(data);
        results.push(response);
    }

    Ok(results)
}
