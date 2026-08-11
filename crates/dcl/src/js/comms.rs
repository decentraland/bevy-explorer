use std::{cell::RefCell, rc::Rc};

use bevy::log::debug;
use common::{
    rpc::{RpcCall, RpcStreamReceiver, RpcStreamSender},
    util::{AsH160, ReportErr},
};
use serde::{Deserialize, Serialize};

use crate::{interface::crdt_context::CrdtContext, RpcCalls, SceneResourceCounters};

use super::State;

const MAX_COMMS_MESSAGE_BYTES: usize = 30_000;
const MAX_NETWORK_MESSAGE_QUEUE: usize = 1024;
const MAX_SEND_MESSAGES_PER_TICK: usize = 512;

// Outbound message budget, reset each tick by `crdt_send_to_renderer`. Enforced in the op (not
// the JS wrapper) so a scene calling the op directly is still bounded.
#[derive(Default)]
pub struct CommsSendBudget {
    pub sent: usize,
}

fn try_spend_budget(state: &mut impl State) -> bool {
    if !state.has::<CommsSendBudget>() {
        state.put(CommsSendBudget::default());
    }
    let budget = state.borrow_mut::<CommsSendBudget>();
    if budget.sent >= MAX_SEND_MESSAGES_PER_TICK {
        return false;
    }
    budget.sent += 1;
    true
}

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

struct BinaryBusReceiver(RpcStreamReceiver<(String, Vec<u8>)>);

pub async fn op_comms_send_string(state: Rc<RefCell<impl State>>, message: String) {
    debug!("op_comms_send_string");
    if message.len() > MAX_COMMS_MESSAGE_BYTES {
        debug!("op_comms_send_string: dropping oversized message");
        return;
    }
    let mut state = state.borrow_mut();
    if !try_spend_budget(&mut *state) {
        debug!("op_comms_send_string: message budget exhausted, dropping");
        return;
    }
    let scene = state.borrow::<CrdtContext>().scene_id.0;
    let mut data = vec![CommsMessageType::String as u8];
    data.extend(message.into_bytes());
    let counters = state.borrow_mut::<SceneResourceCounters>();
    counters.comms_msgs_out += 1;
    counters.comms_bytes_out += (data.len() - 1) as u64;
    state
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::SendMessageBus {
            scene,
            data,
            recipient: None,
        })
        .report();
}

pub async fn op_comms_send_binary_single(
    state: Rc<RefCell<impl State>>,
    message: impl AsRef<[u8]>,
    recipient: Option<String>,
) {
    debug!("op_comms_send_binary_single");
    if message.as_ref().len() > MAX_COMMS_MESSAGE_BYTES {
        debug!("op_comms_send_binary_single: dropping oversized message");
        return;
    }
    let mut state = state.borrow_mut();
    if !try_spend_budget(&mut *state) {
        debug!("op_comms_send_binary_single: message budget exhausted, dropping");
        return;
    }

    let context = state.borrow::<CrdtContext>();
    let scene = context.scene_id.0;
    let mut data = vec![CommsMessageType::Binary as u8];
    data.extend(message.as_ref());

    let counters = state.borrow_mut::<SceneResourceCounters>();
    counters.comms_msgs_out += 1;
    counters.comms_bytes_out += message.as_ref().len() as u64;

    let recipient = recipient.and_then(|r| r.as_h160());

    state
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::SendMessageBus {
            scene,
            data,
            recipient,
        })
        .report();
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
        let (sx, rx) =
            RpcStreamSender::<(String, Vec<u8>)>::channel_with_capacity(MAX_NETWORK_MESSAGE_QUEUE);
        state
            .borrow_mut::<RpcCalls>()
            .push(RpcCall::SubscribeBinaryBus { hash, sender: sx })?;
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
