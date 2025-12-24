use bevy::{platform::sync::Arc, prelude::*};
use serde::{Deserialize, Deserializer};
use wasm_bindgen::{
    convert::{FromWasmAbi, IntoWasmAbi, OptionFromWasmAbi},
    describe::WasmDescribe,
    JsValue,
};

use crate::livekit::web::{traits::GetFromJsValue, DataPacketKind, RemoteParticipant};

// Define structures for the events coming from JavaScript
#[derive(Debug)]
pub enum RoomEvent {
    Connected,
    DataReceived {
        payload: Arc<Vec<u8>>,
        participant: Option<RemoteParticipant>,
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
            Some("dataReceived") => {
                let Some(payload) = Arc::<Vec<u8>>::get_from_js_value(&js_value, "payload") else {
                    error!("RoomEvent::DataReceived did not have payload field.");
                    panic!();
                };
                let participant = RemoteParticipant::get_from_js_value(&js_value, "participant");
                let Some(kind) = DataPacketKind::get_from_js_value(&js_value, "kind") else {
                    error!("RoomEvent::DataReceived did not have kind field.");
                    panic!();
                };
                let topic = String::get_from_js_value(&js_value, "topic");

                RoomEvent::DataReceived {
                    payload,
                    participant,
                    kind,
                    topic,
                }
            }
            Some(tag) => {
                todo!("{tag:?}");
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
