use std::marker::PhantomData;

use bevy::log::{debug, warn};
use common::rpc::RpcCall;
use serde::Serialize;

use crate::{interface::crdt_context::CrdtContext, RpcCalls};

use super::State;

struct EventReceiver<T: EventType> {
    inner: tokio::sync::mpsc::UnboundedReceiver<String>,
    _p: PhantomData<fn() -> T>,
}

trait EventType {
    fn label() -> &'static str;
}

macro_rules! impl_event {
    ($name: ident, $label: expr) => {
        #[derive(Debug)]
        struct $name;
        impl EventType for $name {
            fn label() -> &'static str {
                $label
            }
        }
    };
}

impl_event!(PlayerConnected, "playerConnected");
impl_event!(PlayerDisconnected, "playerDisconnected");
impl_event!(PlayerEnteredScene, "onEnterScene");
impl_event!(PlayerLeftScene, "onLeaveScene");
impl_event!(SceneReady, "sceneStart");
impl_event!(PlayerExpression, "playerExpression");
impl_event!(ProfileChanged, "profileChanged");
impl_event!(RealmChanged, "onRealmChanged");
impl_event!(PlayerClicked, "playerClicked");
impl_event!(MessageBus, "comms");

pub fn op_subscribe(state: &mut impl State, id: &str) {
    macro_rules! register {
        ($id: expr, $state: expr, $marker: ty, $call: expr) => {{
            if id == <$marker as EventType>::label() {
                if $state.has::<EventReceiver<$marker>>() {
                    // already subscribed
                    return;
                }
                let (sx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();

                #[allow(clippy::redundant_closure_call)]
                state.borrow_mut::<RpcCalls>().push($call(sx));

                state.put(EventReceiver::<$marker> {
                    inner: rx,
                    _p: Default::default(),
                });
                debug!("subscribed to {}", <$marker as EventType>::label());
                return;
            }
        }};
    }

    let context = state.borrow::<CrdtContext>();
    let scene = context.scene_id.0;
    let hash = context.hash.clone();

    register!(id, state, PlayerConnected, |sender| {
        RpcCall::SubscribePlayerConnected { sender }
    });
    register!(id, state, PlayerDisconnected, |sender| {
        RpcCall::SubscribePlayerDisconnected { sender }
    });
    register!(id, state, PlayerEnteredScene, |sender| {
        RpcCall::SubscribePlayerEnteredScene { sender, scene }
    });
    register!(id, state, PlayerLeftScene, |sender| {
        RpcCall::SubscribePlayerLeftScene { sender, scene }
    });
    register!(id, state, SceneReady, |sender| {
        RpcCall::SubscribeSceneReady { sender, scene }
    });
    register!(id, state, PlayerExpression, |sender| {
        RpcCall::SubscribePlayerExpression { sender }
    });
    register!(id, state, ProfileChanged, |sender| {
        RpcCall::SubscribeProfileChanged { sender }
    });
    register!(id, state, RealmChanged, |sender| {
        RpcCall::SubscribeRealmChanged { sender }
    });
    register!(id, state, PlayerClicked, |sender| {
        RpcCall::SubscribePlayerClicked { sender }
    });
    register!(id, state, MessageBus, |sender| {
        RpcCall::SubscribeMessageBus { sender, hash }
    });

    warn!("subscribe to unrecognised event {id}");
}

pub fn op_unsubscribe(state: &mut impl State, id: &str) {
    debug!("op_unsubscribe {id}");

    macro_rules! unregister {
        ($id: expr, $state: expr, $marker: ty) => {{
            if id == <$marker as EventType>::label() {
                // removing the receiver will cause the sender to error so it can be cleaned up at the sender side
                state.try_take::<EventReceiver<$marker>>();
                return;
            }
        }};
    }

    unregister!(id, state, PlayerConnected);
    unregister!(id, state, PlayerDisconnected);
    unregister!(id, state, PlayerEnteredScene);
    unregister!(id, state, PlayerLeftScene);
    unregister!(id, state, SceneReady);
    unregister!(id, state, PlayerExpression);
    unregister!(id, state, ProfileChanged);
    unregister!(id, state, RealmChanged);
    unregister!(id, state, PlayerClicked);
    unregister!(id, state, MessageBus);

    warn!("unsubscribe for unrecognised event {id}");
}

#[derive(Serialize)]
pub struct Event {
    generic: EventGeneric,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EventGeneric {
    event_id: String,
    event_data: String,
}

pub fn op_send_batch(state: &mut impl State) -> Vec<Event> {
    debug!("op_send_batch");
    let mut results = Vec::default();

    macro_rules! poll {
        ($state: expr, $marker: ty, $id: expr) => {{
            if let Some(receiver) = state.try_borrow_mut::<EventReceiver<$marker>>() {
                while let Ok(event_data) = receiver.inner.try_recv() {
                    debug!(
                        "received {} event [{}]",
                        <$marker as EventType>::label(),
                        event_data
                    );
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
    poll!(state, RealmChanged, "onRealmChanged");
    poll!(state, PlayerClicked, "playerClicked");
    poll!(state, MessageBus, "comms");

    results
}
