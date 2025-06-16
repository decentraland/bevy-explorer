use std::sync::Arc;

use bevy::{prelude::*, utils::HashMap};
use http::Uri;
use prost::Message;
use serde::Deserialize;
use tokio::sync::{
    mpsc::{Receiver, Sender},
    Mutex,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::{
    global_crdt::{LocalAudioFrame, MicState, PlayerMessage},
    Transport, TransportType,
};
use common::util::AsH160;
use dcl_component::proto_components::kernel::comms::rfc4;

use super::{global_crdt::PlayerUpdate, NetworkMessage};

#[wasm_bindgen(module = "/livekit_web_bindings.js")]
extern "C" {
    #[wasm_bindgen(catch)]
    async fn connect_room(url: &str, token: &str) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch)]
    async fn publish_data(
        room: &JsValue,
        data: &[u8],
        reliable: bool,
        destinations: JsValue,
    ) -> Result<(), JsValue>;

    #[wasm_bindgen(catch)]
    async fn publish_audio_track(room: &JsValue, track: &JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch)]
    async fn unpublish_track(room: &JsValue, sid: &str) -> Result<(), JsValue>;

    #[wasm_bindgen(catch)]
    async fn close_room(room: &JsValue) -> Result<(), JsValue>;

    #[wasm_bindgen(catch)]
    fn create_audio_track(sample_rate: u32, num_channels: u32) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch)]
    fn send_audio_frame(
        track: &JsValue,
        samples: &[f32],
        sample_rate: u32,
        num_channels: u32,
    ) -> Result<(), JsValue>;

    #[wasm_bindgen(catch)]
    fn set_room_event_handler(
        room: &JsValue,
        handler: &Closure<dyn FnMut(JsValue)>,
    ) -> Result<(), JsValue>;

    #[wasm_bindgen(catch)]
    fn set_microphone_enabled(enabled: bool) -> Result<(), JsValue>;

    #[wasm_bindgen(catch)]
    fn is_microphone_available() -> Result<bool, JsValue>;

    #[wasm_bindgen(catch)]
    fn set_participant_spatial_audio(
        participant_identity: &str,
        pan: f32,
        volume: f32,
    ) -> Result<(), JsValue>;

    #[wasm_bindgen(catch)]
    fn set_participant_pan(participant_identity: &str, pan: f32) -> Result<(), JsValue>;

    #[wasm_bindgen(catch)]
    fn set_participant_volume(participant_identity: &str, volume: f32) -> Result<(), JsValue>;

    #[wasm_bindgen(catch)]
    fn get_audio_participants() -> Result<JsValue, JsValue>;
}

pub struct MicPlugin;

impl Plugin for MicPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MicState>();
        app.add_systems(Update, update_mic_state);
    }
}

fn update_mic_state(
    mut mic_state: ResMut<MicState>,
    mut last_enabled: Local<Option<bool>>,
    mut last_available: Local<Option<bool>>,
) {
    // Check if microphone is available in the browser
    let current_available = is_microphone_available().unwrap_or(false);

    // Only update availability if it changed
    if last_available.is_none() || last_available.unwrap() != current_available {
        mic_state.available = current_available;
        *last_available = Some(current_available);
    }

    // Only update microphone enabled state if it changed
    if last_enabled.is_none() || last_enabled.unwrap() != mic_state.enabled {
        if let Err(e) = set_microphone_enabled(mic_state.enabled) {
            warn!("Failed to set microphone state: {:?}", e);
        }
        *last_enabled = Some(mic_state.enabled);
    }
}

pub fn livekit_handler_inner(
    transport_id: Entity,
    remote_address: &str,
    app_rx: Receiver<NetworkMessage>,
    sender: Sender<PlayerUpdate>,
) -> Result<(), anyhow::Error> {
    debug!(">> lk connect async : {}", remote_address);

    let url = Uri::try_from(remote_address).unwrap();
    let address = format!(
        "{}://{}{}",
        url.scheme_str().unwrap_or_default(),
        url.host().unwrap_or_default(),
        url.path()
    );
    let params = HashMap::from_iter(url.query().unwrap_or_default().split('&').flat_map(|par| {
        par.split_once('=')
            .map(|(a, b)| (a.to_owned(), b.to_owned()))
    }));
    debug!("{:?}", params);
    let token = params.get("access_token").cloned().unwrap_or_default();

    // In WASM, we can't block or create threads, so we just spawn the async task
    spawn_local(async move {
        if let Err(e) = run_livekit_session(transport_id, &address, &token, app_rx, sender).await {
            error!("LiveKit session error: {:?}", e);
        }
    });

    Ok(())
}

async fn run_livekit_session(
    transport_id: Entity,
    address: &str,
    token: &str,
    mut app_rx: Receiver<NetworkMessage>,
    sender: Sender<PlayerUpdate>,
) -> Result<(), anyhow::Error> {
    loop {
        // Check if sender is closed (indicates we should stop)
        if sender.is_closed() {
            debug!("Sender closed, stopping LiveKit connection attempts");
            break;
        }

        match connect_and_handle_session(transport_id, address, token, &mut app_rx, &sender).await {
            Ok(_) => {
                debug!("LiveKit session ended normally");
                // Check if we should reconnect
                if sender.is_closed() {
                    break;
                }
                // Session ended but sender still open, might need to reconnect
                // Wait a bit before reconnecting
                gloo_timers::future::TimeoutFuture::new(1000).await;
            }
            Err(e) => {
                error!("LiveKit session error: {:?}", e);

                // Check again if sender is closed before retrying
                if sender.is_closed() {
                    debug!("Sender closed during error, stopping LiveKit connection attempts");
                    break;
                }

                // Wait before retrying
                gloo_timers::future::TimeoutFuture::new(1000).await;
            }
        }
    }

    Ok(())
}

async fn connect_and_handle_session(
    transport_id: Entity,
    address: &str,
    token: &str,
    app_rx: &mut Receiver<NetworkMessage>,
    sender: &Sender<PlayerUpdate>,
) -> Result<(), anyhow::Error> {
    let room = connect_room(address, token)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect room: {:?}", e))?;

    let sender_clone = sender.clone();

    // Set up event handler
    let event_handler = Closure::wrap(Box::new(move |event: JsValue| {
        let sender = sender_clone.clone();

        spawn_local(async move {
            handle_room_event(event, transport_id, sender).await;
        });
    }) as Box<dyn FnMut(JsValue)>);

    set_room_event_handler(&room, &event_handler)
        .map_err(|e| anyhow::anyhow!("Failed to set event handler: {:?}", e))?;

    // Keep the closure alive
    event_handler.forget();

    // Microphone is handled entirely in JavaScript

    // Handle outgoing messages
    loop {
        let message = app_rx.recv().await;
        let Some(outgoing) = message else {
            debug!("App pipe broken, exiting loop");
            break;
        };

        let destinations = if let Some(address) = outgoing.recipient {
            js_sys::Array::of1(&JsValue::from_str(&format!("{:#x}", address)))
        } else {
            js_sys::Array::new()
        };

        if let Err(e) = publish_data(
            &room,
            &outgoing.data,
            !outgoing.unreliable,
            destinations.into(),
        )
        .await
        {
            warn!("Failed to publish data: {:?}", e);
            break;
        }
    }

    close_room(&room)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to close room: {:?}", e))?;

    Ok(())
}

// Define structures for the events coming from JavaScript
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum RoomEvent {
    DataReceived {
        participant: Participant,
        payload: serde_bytes::ByteBuf,
    },
    TrackSubscribed {
        participant: Participant,
    },
    TrackUnsubscribed {
        participant: Participant,
    },
    ParticipantConnected {
        participant: Participant,
    },
    ParticipantDisconnected {
        participant: Participant,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Participant {
    identity: String,
    #[serde(default)]
    metadata: String,
}

async fn handle_room_event(event: JsValue, transport_id: Entity, sender: Sender<PlayerUpdate>) {
    // Try to deserialize the event using serde_wasm_bindgen
    let event_result: Result<RoomEvent, _> = serde_wasm_bindgen::from_value(event);

    match event_result {
        Ok(room_event) => match room_event {
            RoomEvent::DataReceived {
                payload,
                participant,
            } => {
                if let Some(address) = participant.identity.as_h160() {
                    if let Ok(packet) = rfc4::Packet::decode(payload.as_slice()) {
                        if let Some(message) = packet.message {
                            let _ = sender
                                .send(PlayerUpdate {
                                    transport_id,
                                    message: PlayerMessage::PlayerData(message),
                                    address,
                                })
                                .await;
                        }
                    }
                }
            }
            RoomEvent::TrackSubscribed { .. } => {
                debug!("Track subscribed event - audio is handled in JavaScript");
            }
            RoomEvent::TrackUnsubscribed { .. } => {
                debug!("Track unsubscribed event");
            }
            RoomEvent::ParticipantConnected { participant } => {
                if let Some(address) = participant.identity.as_h160() {
                    if !participant.metadata.is_empty() {
                        let _ = sender
                            .send(PlayerUpdate {
                                transport_id,
                                message: PlayerMessage::MetaData(participant.metadata),
                                address,
                            })
                            .await;
                    }
                }
            }
            RoomEvent::ParticipantDisconnected { .. } => {
                debug!("Participant disconnected");
            }
        },
        Err(e) => {
            warn!("Failed to parse room event: {:?}", e);
        }
    }
}

// Public API for spatial audio control
pub fn update_participant_spatial_audio(participant_identity: &str, pan: f32, volume: f32) {
    if let Err(e) = set_participant_spatial_audio(participant_identity, pan, volume) {
        warn!(
            "Failed to set spatial audio for {}: {:?}",
            participant_identity, e
        );
    }
}

pub fn update_participant_pan(participant_identity: &str, pan: f32) {
    if let Err(e) = set_participant_pan(participant_identity, pan) {
        warn!("Failed to set pan for {}: {:?}", participant_identity, e);
    }
}

pub fn update_participant_volume(participant_identity: &str, volume: f32) {
    if let Err(e) = set_participant_volume(participant_identity, volume) {
        warn!("Failed to set volume for {}: {:?}", participant_identity, e);
    }
}
