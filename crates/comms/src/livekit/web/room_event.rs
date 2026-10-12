use bevy::{platform::sync::Arc, prelude::*};
use wasm_bindgen::JsValue;

use crate::livekit::web::{
    host::Registry, ConnectionQuality, ConnectionState, DataPacketKind, DisconnectReason,
    Participant, RemoteParticipant, RemoteTrack, RemoteTrackPublication, RoomId,
};

// Define structures for the events coming from JavaScript
#[derive(Debug)]
pub enum RoomEvent {
    Connected {
        participants_with_tracks: Vec<(RemoteParticipant, Vec<RemoteTrackPublication>)>,
    },
    Disconnected {
        reason: DisconnectReason,
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
    ConnectionQualityChanged {
        quality: ConnectionQuality,
        participant: Participant,
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
        track: RemoteTrack,
        publication: RemoteTrackPublication,
        participant: RemoteParticipant,
    },
    TrackUnsubscribed {
        // Note: The browser livekit docs say that the first parameter is a Livekit.Track,
        // not a Livekit.RemoteTrack, verify if there is ever an event with a local
        // track
        track: RemoteTrack,
        publication: RemoteTrackPublication,
        participant: RemoteParticipant,
    },
    ActiveSpeakersChanged {
        speakers: Vec<Participant>,
    },
}

// Page side: the conversion of the event objects `set_room_event_handler` (livekit_web_bindings.js)
// emits into plain data. A malformed event is logged and dropped (a panic would trap the page).

fn get(js_value: &JsValue, key: &str) -> Option<JsValue> {
    js_sys::Reflect::get(js_value, &JsValue::from(key))
        .ok()
        .filter(|value| !(value.is_null() || value.is_undefined()))
}

fn get_string(js_value: &JsValue, key: &str) -> Option<String> {
    get(js_value, key).and_then(|value| value.as_string())
}

fn get_array(js_value: &JsValue, key: &str) -> Option<js_sys::Array> {
    get(js_value, key).map(Into::<js_sys::Array>::into)
}

fn get_payload(js_value: &JsValue, key: &str) -> Option<Arc<Vec<u8>>> {
    let payload = get(js_value, key)?;
    Some(Arc::new(js_sys::Uint8Array::new(&payload).to_vec()))
}

fn get_data_packet_kind(js_value: &JsValue, key: &str) -> Option<DataPacketKind> {
    match get(js_value, key)?.as_f64()? as u32 {
        0 => Some(DataPacketKind::Reliable),
        1 => Some(DataPacketKind::Lossy),
        int => {
            error!("Should always be 0 for Reliable or 1 for Lossy, but was {int}.");
            None
        }
    }
}

fn get_connection_quality(js_value: &JsValue, key: &str) -> Option<ConnectionQuality> {
    let quality = get_string(js_value, key)?;
    match quality.as_str() {
        "excellent" => Some(ConnectionQuality::Excellent),
        "good" => Some(ConnectionQuality::Good),
        "poor" => Some(ConnectionQuality::Poor),
        "lost" => Some(ConnectionQuality::Lost),
        other => {
            error!("Invalid ConnectionQuality '{other}'.");
            None
        }
    }
}

fn get_disconnect_reason(js_value: &JsValue, key: &str) -> Option<DisconnectReason> {
    let reason = get(js_value, key)?.as_f64()? as i32;
    let parsed = DisconnectReason::from_i32(reason);
    if parsed.is_none() {
        error!("Not a valid DisconnectReason: {reason}.");
    }
    parsed
}

fn get_remote_participant(
    js_value: &JsValue,
    key: &str,
    registry: &mut Registry,
) -> Option<RemoteParticipant> {
    get(js_value, key).map(|participant| registry.remote_participant(&participant))
}

fn get_participant(
    js_value: &JsValue,
    key: &str,
    room: RoomId,
    registry: &mut Registry,
) -> Option<Participant> {
    get(js_value, key).map(|participant| registry.participant(&participant, room))
}

fn get_publication(
    js_value: &JsValue,
    key: &str,
    registry: &mut Registry,
) -> Option<RemoteTrackPublication> {
    get(js_value, key).map(|publication| registry.remote_track_publication(&publication))
}

fn get_remote_track(js_value: &JsValue, key: &str, registry: &mut Registry) -> Option<RemoteTrack> {
    get(js_value, key).and_then(|track| registry.remote_track(&track))
}

impl RoomEvent {
    pub(super) fn from_js(
        js_value: &JsValue,
        room: RoomId,
        registry: &mut Registry,
    ) -> Option<Self> {
        let tag = get_string(js_value, "type");

        let event = match tag.as_deref() {
            Some("connected") => {
                let Some(participants_with_tracks) =
                    get_array(js_value, "participants_with_tracks")
                else {
                    error!("RoomEvent::Connected did not have participants_with_tracks field.");
                    return None;
                };

                let participants_with_tracks = participants_with_tracks
                    .iter()
                    .filter_map(|js_object: JsValue| {
                        let Some(participant) =
                            get_remote_participant(&js_object, "participant", registry)
                        else {
                            error!(
                                "Object in participants_with_tracks array of RoomEvent::Connected\
                        did not have participant field."
                            );
                            return None;
                        };
                        let Some(publications) = get_array(&js_object, "tracks") else {
                            error!(
                                "Object in participants_with_tracks array of RoomEvent::Connected\
                        did not have tracks field."
                            );
                            return None;
                        };

                        Some((
                            participant,
                            publications
                                .iter()
                                .map(|publication| registry.remote_track_publication(&publication))
                                .collect(),
                        ))
                    })
                    .collect::<Vec<_>>();

                RoomEvent::Connected {
                    participants_with_tracks,
                }
            }
            Some("disconnected") => {
                let Some(reason) = get_disconnect_reason(js_value, "disconnectReason") else {
                    error!("RoomEvent::Disconnected did not have disconnectReason field.");
                    return None;
                };

                RoomEvent::Disconnected { reason }
            }
            Some("connectionStateChanged") => {
                let Some(state) = get_string(js_value, "state") else {
                    error!("RoomEvent::ConnectionStateChanged did not have state field.");
                    return None;
                };
                let state = match state.as_str() {
                    "connecting" => ConnectionState::Reconnecting,
                    "connected" => ConnectionState::Connected,
                    "signalReconnecting" | "reconnecting" => ConnectionState::Reconnecting,
                    "disconnected" => ConnectionState::Disconnected,
                    _ => {
                        error!("Invalid ConnectionState '{state}'.");
                        return None;
                    }
                };
                RoomEvent::ConnectionStateChanged(state)
            }
            Some("dataReceived") => {
                let Some(payload) = get_payload(js_value, "payload") else {
                    error!("RoomEvent::DataReceived did not have payload field.");
                    return None;
                };
                let participant = get_remote_participant(js_value, "participant", registry);
                let Some(kind) = get_data_packet_kind(js_value, "kind") else {
                    error!("RoomEvent::DataReceived did not have kind field.");
                    return None;
                };
                let topic = get_string(js_value, "topic");

                RoomEvent::DataReceived {
                    payload,
                    participant,
                    kind,
                    topic,
                }
            }
            Some("participantConnected") => {
                let Some(participant) = get_remote_participant(js_value, "participant", registry)
                else {
                    error!("RoomEvent::ParticipantConnected did not have participant field.");
                    return None;
                };
                RoomEvent::ParticipantConnected(participant)
            }
            Some("participantDisconnected") => {
                let Some(participant) = get_remote_participant(js_value, "participant", registry)
                else {
                    error!("RoomEvent::ParticipantDisconnected did not have participant field.");
                    return None;
                };
                registry.release(participant.id);
                RoomEvent::ParticipantDisconnected(participant)
            }
            Some("participantMetadataChanged") => {
                let Some(participant) = get_participant(js_value, "participant", room, registry)
                else {
                    error!("RoomEvent::ParticipantDisconnected did not have participant field.");
                    return None;
                };
                let Some(old_metadata) = get_string(js_value, "old_metadata") else {
                    error!(
                        "RoomEvent::ParticipantMetadataChanged did not have old_metadata field."
                    );
                    return None;
                };
                let Some(metadata) = get_string(js_value, "metadata") else {
                    error!("RoomEvent::ParticipantMetadataChanged did not have metadata field.");
                    return None;
                };
                RoomEvent::ParticipantMetadataChanged {
                    participant,
                    old_metadata,
                    metadata,
                }
            }
            Some("connectionQualityChanged") => {
                let Some(quality) = get_connection_quality(js_value, "connection_quality") else {
                    error!("RoomEvent::ConnectionQualityChanged did not have quality field.");
                    return None;
                };
                let Some(participant) = get_participant(js_value, "participant", room, registry)
                else {
                    error!("RoomEvent::ConnectionQualityChanged did not have participant field.");
                    return None;
                };
                RoomEvent::ConnectionQualityChanged {
                    quality,
                    participant,
                }
            }
            Some("trackPublished") => {
                let Some(publication) = get_publication(js_value, "publication", registry) else {
                    error!("RoomEvent::TrackPublished did not have publication field.");
                    return None;
                };
                let Some(participant) = get_remote_participant(js_value, "participant", registry)
                else {
                    error!("RoomEvent::TrackPublished did not have participant field.");
                    return None;
                };
                RoomEvent::TrackPublished {
                    publication,
                    participant,
                }
            }
            Some("trackUnpublished") => {
                let Some(publication) = get_publication(js_value, "publication", registry) else {
                    error!("RoomEvent::TrackUnpublished did not have publication field.");
                    return None;
                };
                let Some(participant) = get_remote_participant(js_value, "participant", registry)
                else {
                    error!("RoomEvent::TrackUnpublished did not have participant field.");
                    return None;
                };
                registry.release(publication.id);
                RoomEvent::TrackUnpublished {
                    publication,
                    participant,
                }
            }
            Some("trackSubscribed") => {
                let Some(track) = get_remote_track(js_value, "track", registry) else {
                    error!("RoomEvent::TrackSubscribed did not have track field.");
                    return None;
                };
                let Some(publication) = get_publication(js_value, "publication", registry) else {
                    error!("RoomEvent::TrackSubscribed did not have publication field.");
                    return None;
                };
                let Some(participant) = get_remote_participant(js_value, "participant", registry)
                else {
                    error!("RoomEvent::TrackSubscribed did not have participant field.");
                    return None;
                };
                RoomEvent::TrackSubscribed {
                    track,
                    publication,
                    participant,
                }
            }
            Some("trackUnsubscribed") => {
                let Some(track) = get_remote_track(js_value, "track", registry) else {
                    error!("RoomEvent::TrackUnsubscribed did not have track field.");
                    return None;
                };
                let Some(mut publication) = get_publication(js_value, "publication", registry)
                else {
                    error!("RoomEvent::TrackUnsubscribed did not have publication field.");
                    return None;
                };
                // livekit-client emits this before it clears the publication's track
                publication.track = None;
                let Some(participant) = get_remote_participant(js_value, "participant", registry)
                else {
                    error!("RoomEvent::TrackUnsubscribed did not have participant field.");
                    return None;
                };
                registry.release(track.id());
                RoomEvent::TrackUnsubscribed {
                    track,
                    publication,
                    participant,
                }
            }
            Some("activeSpeakersChanged") => {
                let Some(speakers) = get_array(js_value, "speakers") else {
                    error!("RoomEvent::ActiveSpeakersChanged did not have speakers field.");
                    return None;
                };
                let speakers = speakers
                    .iter()
                    .map(|speaker| registry.participant(&speaker, room))
                    .collect();
                RoomEvent::ActiveSpeakersChanged { speakers }
            }
            Some(tag) => {
                error!("Unhandled RoomEvent {tag:?} {js_value:?}");
                return None;
            }
            None => {
                error!("RoomEvent's `type` was not a string, was {tag:?}.");
                return None;
            }
        };
        Some(event)
    }
}
