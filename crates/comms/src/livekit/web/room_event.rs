use bevy::{platform::sync::Arc, prelude::*};
use wasm_bindgen::{
    convert::{FromWasmAbi, IntoWasmAbi, OptionFromWasmAbi},
    JsValue,
};

use crate::livekit::web::{
    traits::GetFromJsValue, ConnectionState, DataPacketKind, Participant, RemoteParticipant,
    RemoteTrackPublication,
};

// Define structures for the events coming from JavaScript
#[derive(Debug)]
pub enum RoomEvent {
    Connected {
        participants_with_tracks: Vec<(RemoteParticipant, Vec<RemoteTrackPublication>)>,
    },
    ConnectionStateChanged(ConnectionState),
    DataReceived {
        payload: Arc<Vec<u8>>,
        participant: Option<RemoteParticipant>,
        kind: DataPacketKind,
        topic: Option<String>,
    },
    ParticipantConnected(RemoteParticipant),
    ParticipantDisconnected(RemoteParticipant),
    ParticipantMetadataChanged {
        participant: Participant,
        old_metadata: String,
        metadata: String,
    },
    TrackPublished {
        publication: RemoteTrackPublication,
        participant: RemoteParticipant,
    },
    TrackUnpublished {
        publication: RemoteTrackPublication,
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
            Some("connected") => {
                let Some(participants_with_tracks) =
                    js_sys::Reflect::get(&js_value, &JsValue::from("participants_with_tracks"))
                        .ok()
                        .map(Into::<js_sys::Array>::into)
                else {
                    error!("RoomEvent::Connected did not have participants_with_tracks field.");
                    panic!();
                };

                let participants_with_tracks = participants_with_tracks
                    .iter()
                    .map(|js_object: JsValue| {
                        let Some(participant) =
                            RemoteParticipant::get_from_js_value(&js_object, "participant")
                        else {
                            error!(
                                "Object in participants_with_tracks array of RoomEvent::Connected\
                        did not have participant field."
                            );
                            panic!();
                        };
                        let Some(publications) =
                            js_sys::Reflect::get(&js_object, &JsValue::from("tracks"))
                                .ok()
                                .map(Into::<js_sys::Array>::into)
                        else {
                            error!(
                                "Object in participants_with_tracks array of RoomEvent::Connected\
                        did not have tracks field."
                            );
                            panic!();
                        };

                        (
                            participant,
                            publications
                                .iter()
                                .map(|publication| RemoteTrackPublication::from(publication))
                                .collect(),
                        )
                    })
                    .collect::<Vec<_>>();

                RoomEvent::Connected {
                    participants_with_tracks,
                }
            }
            Some("connectionStateChanged") => {
                let Some(state) = String::get_from_js_value(&js_value, "state") else {
                    error!("RoomEvent::ConnectionStateChanged did not have state field.");
                    panic!();
                };
                let state = match state.as_str() {
                    "connecting" => ConnectionState::Reconnecting,
                    "connected" => ConnectionState::Connected,
                    "reconnecting" => ConnectionState::Reconnecting,
                    "disconnected" => ConnectionState::Disconnected,
                    _ => {
                        error!("Invalid ConnectionState '{state}'.");
                        panic!()
                    }
                };
                RoomEvent::ConnectionStateChanged(state)
            }
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
            Some("participantConnected") => {
                let Some(participant) =
                    RemoteParticipant::get_from_js_value(&js_value, "participant")
                else {
                    error!("RoomEvent::ParticipantConnected did not have participant field.");
                    panic!();
                };
                RoomEvent::ParticipantConnected(participant)
            }
            Some("participantDisconnected") => {
                let Some(participant) =
                    RemoteParticipant::get_from_js_value(&js_value, "participant")
                else {
                    error!("RoomEvent::ParticipantDisconnected did not have participant field.");
                    panic!();
                };
                RoomEvent::ParticipantDisconnected(participant)
            }
            Some("participantMetadataChanged") => {
                let Some(participant) = Participant::get_from_js_value(&js_value, "participant")
                else {
                    error!("RoomEvent::ParticipantDisconnected did not have participant field.");
                    panic!();
                };
                let Some(old_metadata) = String::get_from_js_value(&js_value, "old_metadata")
                else {
                    error!(
                        "RoomEvent::ParticipantMetadataChanged did not have old_metadata field."
                    );
                    panic!();
                };
                let Some(metadata) = String::get_from_js_value(&js_value, "metadata") else {
                    error!("RoomEvent::ParticipantMetadataChanged did not have metadata field.");
                    panic!();
                };
                RoomEvent::ParticipantMetadataChanged {
                    participant,
                    old_metadata,
                    metadata,
                }
            }
            Some("trackPublished") => {
                let Some(publication) =
                    RemoteTrackPublication::get_from_js_value(&js_value, "publication")
                else {
                    error!("RoomEvent::TrackPublished did not have publication field.");
                    panic!();
                };
                let Some(participant) =
                    RemoteParticipant::get_from_js_value(&js_value, "participant")
                else {
                    error!("RoomEvent::TrackPublished did not have participant field.");
                    panic!();
                };
                RoomEvent::TrackPublished {
                    publication,
                    participant,
                }
            }
            Some("trackUnpublished") => {
                let Some(publication) =
                    RemoteTrackPublication::get_from_js_value(&js_value, "publication")
                else {
                    error!("RoomEvent::TrackUnpublished did not have publication field.");
                    panic!();
                };
                let Some(participant) =
                    RemoteParticipant::get_from_js_value(&js_value, "participant")
                else {
                    error!("RoomEvent::TrackUnpublished did not have participant field.");
                    panic!();
                };
                RoomEvent::TrackUnpublished {
                    publication,
                    participant,
                }
            }
            Some(tag) => {
                todo!("{tag:?} {js_value:?}");
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
