use std::marker::PhantomData;

use bevy::prelude::warn;
use common::rpc::RpcCall;
use deno_core::{op, Op, OpDecl, OpState};
use serde::Serialize;

use crate::{interface::crdt_context::CrdtContext, RpcCalls};

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
struct PlayerEnteredScene;
struct PlayerLeftScene;
struct SceneReady;
struct PlayerExpression;
struct ProfileChanged;

#[op]
fn op_subscribe(state: &mut OpState, id: &str) {
    macro_rules! register {
        ($state: expr, $marker: ty, $call: expr) => {{
            if $state.has::<EventReceiver<$marker>>() {
                // already subscribed
                return;
            }
            let (sx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();

            state.borrow_mut::<RpcCalls>().push($call(sx));

            state.put(EventReceiver::<$marker> {
                inner: rx,
                _p: Default::default(),
            });
        }};
    }

    let scene = state.borrow::<CrdtContext>().scene_id.0;

    // clippy is wrong https://github.com/rust-lang/rust-clippy/issues/1553
    #[allow(clippy::redundant_closure_call)]
    match id {
        "playerConnected" => register!(state, PlayerConnected, |sender| {
            RpcCall::SubscribePlayerConnected { sender }
        }),
        "playerDisconnected" => register!(state, PlayerDisconnected, |sender| {
            RpcCall::SubscribePlayerDisconnected { sender }
        }),
        "onEnterScene" => register!(state, PlayerEnteredScene, |sender| {
            RpcCall::SubscribePlayerEnteredScene { sender, scene }
        }),
        "onLeaveScene" => register!(state, PlayerLeftScene, |sender| {
            RpcCall::SubscribePlayerLeftScene { sender, scene }
        }),
        "sceneStart" => register!(state, SceneReady, |sender| {
            RpcCall::SubscribeSceneReady { sender, scene }
        }),
        "playerExpression" => register!(state, PlayerExpression, |sender| {
            RpcCall::SubscribePlayerExpression { sender }
        }),
        "profileChanged" => register!(state, ProfileChanged, |sender| {
            RpcCall::SubscribeProfileChanged { sender }
        }),
        _ => warn!("subscribe to unrecognised event {id}"),
    }
}

#[op]
fn op_unsubscribe(state: &mut OpState, id: &str) {
    macro_rules! unregister {
        ($state: expr, $marker: ty) => {{
            // removing the receiver will cause the sender to error so it can be cleaned up at the sender side
            state.try_take::<EventReceiver<$marker>>();
        }};
    }

    match id {
        "playerConnected" => unregister!(state, PlayerConnected),
        "playerDisconnected" => unregister!(state, PlayerDisconnected),
        "onEnterScene" => unregister!(state, PlayerEnteredScene),
        "onLeaveScene" => unregister!(state, PlayerLeftScene),
        "sceneStart" => unregister!(state, SceneReady),
        "playerExpression" => unregister!(state, PlayerExpression),
        "profileChanged" => unregister!(state, ProfileChanged),
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
    poll!(state, PlayerEnteredScene, "onEnterScene");
    poll!(state, PlayerLeftScene, "onLeaveScene");
    poll!(state, SceneReady, "sceneStart");
    poll!(state, PlayerExpression, "playerExpression");
    poll!(state, ProfileChanged, "profileChanged");

    results
}
