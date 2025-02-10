use std::{cell::RefCell, rc::Rc};

use bevy::log::debug;
use common::{rpc::RpcCall, util::AsH160};
use deno_core::{anyhow, op2, JsBuffer, OpDecl, OpState};
use serde::{Deserialize, Serialize};

use crate::{interface::crdt_context::CrdtContext, RpcCalls};

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

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![
        op_comms_send_string(),
        op_comms_send_binary_single(),
        op_comms_recv_binary(),
    ]
}

struct BinaryBusReceiver(tokio::sync::mpsc::UnboundedReceiver<(String, Vec<u8>)>);

#[op2(async)]
async fn op_comms_send_string(state: Rc<RefCell<OpState>>, #[string] message: String) {
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

/*#[op2(async)]
#[serde]
async fn op_comms_send_binary(
    state: Rc<RefCell<OpState>>,
    #[serde] messages: Vec<JsBuffer>,
) -> Result<Vec<Vec<u8>>, anyhow::Error> {
    debug!("op_comms_send_binary");
    let mut state = state.borrow_mut();

    let context = state.borrow::<CrdtContext>();
    let scene = context.scene_id.0;
    let hash = context.hash.clone();

    let mut results = Vec::default();

    for message in messages {
        let mut data = vec![CommsMessageType::Binary as u8];
        data.extend(message.as_ref());
        state
            .borrow_mut::<RpcCalls>()
            .push(RpcCall::SendMessageBus { scene, data });
    }

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
}*/

#[op2(async)]
async fn op_comms_send_binary_single(
    state: Rc<RefCell<OpState>>,
    #[buffer(detach)] message: JsBuffer,
    #[string] recipient: String,
) {
    debug!("op_comms_send_binary_single");
    let mut state = state.borrow_mut();

    let context = state.borrow::<CrdtContext>();
    let scene = context.scene_id.0;
    let mut data = vec![CommsMessageType::Binary as u8];
    data.extend(message.as_ref());

    let recipient = recipient.as_h160();

    state
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::SendMessageBus {
            scene,
            data,
            recipient,
        });
}

#[op2(async)]
#[serde]
async fn op_comms_recv_binary(state: Rc<RefCell<OpState>>) -> Result<Vec<Vec<u8>>, anyhow::Error> {
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
