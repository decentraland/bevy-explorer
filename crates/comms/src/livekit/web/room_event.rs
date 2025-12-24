use bevy::{platform::sync::Arc, prelude::*};
use wasm_bindgen::{convert::{FromWasmAbi, IntoWasmAbi, OptionFromWasmAbi}, JsValue, describe::WasmDescribe};
use serde::{Deserialize, Deserializer};

use crate::livekit::web::{DataPacketKind, RemoteParticipant};

// Define structures for the events coming from JavaScript
#[derive(Debug)]
pub enum RoomEvent {
    Connected,
    DataReceived {
        payload: Arc<Vec<u8>>,
        participant: RemoteParticipant,
        kind: DataPacketKind,
        topic: Option<String>,
    },
    TrackPublished {
        room_name: String,
        kind: String,
        participant: RemoteParticipant,
    },
    TrackUnpublished {
        room_name: String,
        kind: String,
        participant: RemoteParticipant,
    },
    TrackSubscribed {
        room_name: String,
        participant: RemoteParticipant,
    },
    TrackUnsubscribed {
        room_name: String,
        participant: RemoteParticipant,
    },
    ParticipantConnected {
        room_name: String,
        participant: RemoteParticipant,
    },
    ParticipantDisconnected {
        room_name: String,
        participant: RemoteParticipant,
    },
}

impl wasm_bindgen::describe::WasmDescribe for RoomEvent {
    fn describe() {
        JsValue::describe()
    }
}

impl FromWasmAbi for RoomEvent {
    type Abi = <JsValue as IntoWasmAbi>::Abi;

    unsafe fn from_abi(abi: Self::Abi) -> Self {
        let js_value = JsValue::from_abi(abi);
        let tag = js_sys::Reflect::get(&js_value, &JsValue::from("type"))
            .ok()
            .and_then(|tag| tag.as_string());

        match tag.as_deref() {
            Some("connected") => RoomEvent::Connected,
            Some(tag) => {
                let Some(payload) = js_sys::Reflect::get(&js_value, &JsValue::from("payload"))
                    .ok()
                    .and_then(|payload| {
                        serde_wasm_bindgen::from_value::<PayloadIntermediate>(payload).ok()
                    })
                else {
                    error!("RoomEvent::DataReceived did not have payload field.");
                    panic!();
                };
                let Some(participant) =
                    js_sys::Reflect::get(&js_value, &JsValue::from("participant"))
                        .ok()
                        .map(|participant| RemoteParticipant {
                            inner: participant,
                        })
                else {
                    error!("RoomEvent::DataReceived did not have participant field.");
                    panic!();
                };
                let Some(kind) = js_sys::Reflect::get(&js_value, &JsValue::from("kind"))
                    .ok()
                    .and_then(|kind| serde_wasm_bindgen::from_value::<DataPacketKind>(kind).ok())
                else {
                    error!("RoomEvent::DataReceived did not have kind field.");
                    panic!();
                };
                let topic = js_sys::Reflect::get(&js_value, &JsValue::from("topic"))
                    .ok()
                    .and_then(|topic| topic.as_string());
                RoomEvent::DataReceived {
                    payload: payload.0,
                    participant,
                    kind,
                    topic,
                }
            }
            None => {
                error!("RoomEvent's `type` was not a string, was {tag:?}.");
                panic!()
            }
        }
    }
}

impl OptionFromWasmAbi for RoomEvent {
    fn is_none(abi: &Self::Abi) -> bool {
        std::mem::ManuallyDrop::new(unsafe { JsValue::from_abi(*abi) }).is_object()
    }
}

#[derive(Debug)]
struct PayloadIntermediate(Arc<Vec<u8>>);

impl<'de> Deserialize<'de> for PayloadIntermediate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let buf = serde_bytes::ByteBuf::deserialize(deserializer)?;
        Ok(Self(Arc::new(buf.into_vec())))
    }
}
