use std::{cell::RefCell, rc::Rc};

use bevy::log::debug;
use common::rpc::RpcCall;
use deno_core::{anyhow, op, JsBuffer, Op, OpDecl, OpState};
use serde::{Deserialize, Serialize};

use crate::{interface::crdt_context::CrdtContext, js::user_identity::UserEthAddress, RpcCalls};

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
    vec![op_comms_send_string::DECL, op_comms_send_binary::DECL]
}

struct BinaryBusReceiver(tokio::sync::mpsc::UnboundedReceiver<(String, Vec<u8>)>);

#[op]
async fn op_comms_send_string(state: Rc<RefCell<OpState>>, message: String) {
    debug!("op_comms_send_string");
    let mut state = state.borrow_mut();
    let scene = state.borrow::<CrdtContext>().scene_id.0;
    let mut data = vec![CommsMessageType::String as u8];
    data.extend(message.into_bytes());
    state
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::SendMessageBus { scene, data });
}

#[op]
async fn op_comms_send_binary(state: Rc<RefCell<OpState>>, messages: Vec<JsBuffer>) -> Result<Vec<Vec<u8>>, anyhow::Error> {
    debug!("op_comms_send_binary");
    let mut state = state.borrow_mut();

    let context = state.borrow::<CrdtContext>();
    let scene = context.scene_id.0;
    let hash = context.hash.clone();

    let address = state.try_borrow::<UserEthAddress>().ok_or(anyhow::anyhow!("not connected"))?.0.clone().into_bytes();

    let mut results = Vec::default();

    for message in messages {
        let mut data = vec![CommsMessageType::Binary as u8];
        data.extend(message.as_ref());
        state
            .borrow_mut::<RpcCalls>()
            .push(RpcCall::SendMessageBus { scene, data });

        let mut response = vec![address.len() as u8];
        response.extend(&address);
        response.extend(message.as_ref());
        results.push(response);
    }

    if !state.has::<BinaryBusReceiver>() {
        let (sx, rx) = tokio::sync::mpsc::unbounded_channel::<(String, Vec<u8>)>();
        state.borrow_mut::<RpcCalls>().push(RpcCall::SubscribeBinaryBus { hash, sender: sx });
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
