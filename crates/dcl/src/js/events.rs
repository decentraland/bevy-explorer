use std::marker::PhantomData;

use bevy::prelude::warn;
use common::rpc::RpcCall;
use deno_core::{op, Op, OpDecl, OpState};
use serde::Serialize;

use crate::RpcCalls;

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![
        op_subscribe::DECL,
        op_unsubscribe::DECL,
        op_send_batch::DECL,
    ]
}

struct EventReceiver<T> {
    inner: tokio::sync::mpsc::UnboundedReceiver<String>,
    _p: PhantomData<fn() -> T>,
}

struct PlayerConnected;
struct PlayerDisconnected;

#[op]
fn op_subscribe(state: &mut OpState, id: &str) {
    macro_rules! register {
        ($state: expr, $marker: ty, $call: tt) => {{
            if $state.has::<EventReceiver<$marker>>() {
                // already subscribed
                return;
            }
            let (sx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();

            state
                .borrow_mut::<RpcCalls>()
                .push(RpcCall::$call { sender: sx });

            state.put(EventReceiver::<$marker> {
                inner: rx,
                _p: Default::default(),
            });
        }};
    }

    match id {
        "playerConnected" => register!(state, PlayerConnected, SubscribePlayerConnected),
        "playerDisconnected" => register!(state, PlayerDisconnected, SubscribePlayerDisconnected),
        _ => warn!("subscribe to unrecognised event {id}"),
    }
}

#[op]
fn op_unsubscribe(state: &mut OpState, id: &str) {
    macro_rules! unregister {
        ($state: expr, $marker: ty) => {{
            state.try_take::<EventReceiver<$marker>>();
        }};
    }

    match id {
        "playerConnected" => unregister!(state, PlayerConnected),
        "playerDisconnected" => unregister!(state, PlayerDisconnected),
        _ => warn!("subscribe to unrecognised event {id}"),
    }
}

#[derive(Serialize)]
struct Event {
    generic: EventGeneric,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EventGeneric {
    event_id: String,
    event_data: String,
}

#[op]
fn op_send_batch(state: &mut OpState) -> Vec<Event> {
    let mut results = Vec::default();

    macro_rules! poll {
        ($state: expr, $marker: ty, $id: expr) => {{
            if let Some(receiver) = state.try_borrow_mut::<EventReceiver<$marker>>() {
                while let Ok(event_data) = receiver.inner.try_recv() {
                    results.push(Event {
                        generic: EventGeneric {
                            event_id: $id.to_owned(),
                            event_data,
                        },
                    });
                }
            }
        }};
    }

    poll!(state, PlayerConnected, "playerConnected");
    poll!(state, PlayerDisconnected, "playerDisconnected");

    results
}
