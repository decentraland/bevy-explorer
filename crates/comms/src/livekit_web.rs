use bevy::{
    platform::{collections::HashMap, hash::FixedHasher},
    prelude::*,
};
use ethers_core::types::H160;
use http::Uri;
use prost::Message;
use serde::Deserialize;
use tokio::sync::mpsc::{Receiver, Sender};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::{
    global_crdt::{
        ChannelControl, GlobalCrdtState, NetworkUpdate, NonPlayerUpdate, PlayerMessage,
        PlayerUpdate,
    },
    livekit_room::{LivekitConnection, LivekitTransport},
    NetworkMessage, NetworkMessageRecipient,
};
use common::{structs::MicState, util::AsH160};
use dcl_component::proto_components::kernel::comms::rfc4;

#[wasm_bindgen(module = "/livekit_web_bindings.js")]
extern "C" {
    #[wasm_bindgen(catch)]
    async fn connect_room(
        url: &str,
        token: &str,
        handler: &Closure<dyn FnMut(JsValue)>,
    ) -> Result<JsValue, JsValue>;

    #[wasm_bindgen]
    fn room_name(room: &JsValue) -> String;

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

    #[wasm_bindgen(catch)]
    fn subscribe_channel(
        room_name: &str,
        participant_identity: &str,
        subscribe: bool,
    ) -> Result<(), JsValue>;

    #[wasm_bindgen(catch)]
    fn streamer_subscribe_channel(
        room_name: &str,
        subscribe_audio: bool,
        subscribe_video: bool,
    ) -> Result<(), JsValue>;
}

pub struct MicPlugin;

impl Plugin for MicPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MicState>();
        app.add_systems(Update, update_mic_state);
        app.add_systems(Update, locate_foreign_streams);
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

#[allow(clippy::type_complexity)]
pub fn connect_livekit(
    mut commands: Commands,
    mut new_livekits: Query<(Entity, &mut LivekitTransport), Without<LivekitConnection>>,
    player_state: Res<GlobalCrdtState>,
) {
    for (transport_id, mut new_transport) in new_livekits.iter_mut() {
        debug!("spawn lk connect");
        let remote_address = new_transport.address.to_owned();
        let receiver = new_transport.receiver.take().unwrap();
        let control_receiver = new_transport.control_receiver.take().unwrap();
        let sender = player_state.get_sender();

        // For WASM, we directly call the handler which will spawn the async task
        if let Err(e) = livekit_handler_inner(
            transport_id,
            &remote_address,
            receiver,
            control_receiver,
            sender,
        ) {
            warn!("Failed to start livekit connection: {e}");
        }

        commands.entity(transport_id).try_insert(LivekitConnection);
    }
}

fn livekit_handler_inner(
    transport_id: Entity,
    remote_address: &str,
    app_rx: Receiver<NetworkMessage>,
    control_rx: Receiver<ChannelControl>,
    sender: Sender<NetworkUpdate>,
) -> Result<(), anyhow::Error> {
    debug!(">> lk connect async : {}", remote_address);

    let url = Uri::try_from(remote_address).unwrap();
    let address = format!(
        "{}://{}{}",
        url.scheme_str().unwrap_or_default(),
        url.host().unwrap_or_default(),
        url.path()
    );
    let params: HashMap<_, _, FixedHasher> =
        HashMap::from_iter(url.query().unwrap_or_default().split('&').flat_map(|par| {
            par.split_once('=')
                .map(|(a, b)| (a.to_owned(), b.to_owned()))
        }));
    debug!("{:?}", params);
    let token = params.get("access_token").cloned().unwrap_or_default();

    // In WASM, we can't block or create threads, so we just spawn the async task
    spawn_local(async move {
        if let Err(e) =
            run_livekit_session(transport_id, &address, &token, app_rx, control_rx, sender).await
        {
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
    mut control_rx: Receiver<ChannelControl>,
    sender: Sender<NetworkUpdate>,
) -> Result<(), anyhow::Error> {
    loop {
        // Check if sender is closed (indicates we should stop)
        if sender.is_closed() {
            debug!("Sender closed, stopping LiveKit connection attempts");
            break;
        }

        match connect_and_handle_session(
            transport_id,
            address,
            token,
            &mut app_rx,
            &mut control_rx,
            &sender,
        )
        .await
        {
            Ok(_) => {
                debug!("LiveKit session ended normally");
                // Check if we should reconnect
                if app_rx.is_closed() {
                    break;
                }
                // Session ended but receiver still open, might need to reconnect
                // Wait a bit before reconnecting
                gloo_timers::future::TimeoutFuture::new(1000).await;
            }
            Err(e) => {
                error!("LiveKit session error: {:?}", e);

                // Check again if receiver is closed before retrying
                if app_rx.is_closed() {
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
    control_rx: &mut Receiver<ChannelControl>,
    sender: &Sender<NetworkUpdate>,
) -> Result<(), anyhow::Error> {
    let sender_clone = sender.clone();

    // Set up event handler
    let event_handler = Closure::wrap(Box::new(move |event: JsValue| {
        let sender = sender_clone.clone();

        spawn_local(async move {
            handle_room_event(event, transport_id, sender).await;
        });
    }) as Box<dyn FnMut(JsValue)>);

    let room = connect_room(address, token, &event_handler)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect room: {:?}", e))?;
    let room_name = room_name(&room);

    // Handle outgoing messages
    'stream: loop {
        tokio::select!(
            message = app_rx.recv() => {
                let Some(outgoing) = message else {
                    debug!("App pipe broken, exiting loop");
                    break 'stream;
                };

                let destination_identities = match outgoing.recipient {
                    NetworkMessageRecipient::All => js_sys::Array::new(),
                    NetworkMessageRecipient::Peer(address) => js_sys::Array::of1(&JsValue::from_str(&format!("{address:#x}"))),
                    NetworkMessageRecipient::AuthServer => js_sys::Array::of1(&JsValue::from_str("authoritative-server")),
                };

                if let Err(e) = publish_data(
                    &room,
                    &outgoing.data,
                    !outgoing.unreliable,
                    destination_identities.into(),
                )
                .await
                {
                    warn!("Failed to publish data: {:?}", e);
                    break 'stream;
                }
            }
            control = control_rx.recv() => {
                let Some(control) = control else {
                    debug!("app pipe broken, exiting loop");
                    break 'stream;
                };

                match control {
                    ChannelControl::VoiceSubscribe(address, _) => participant_audio_subscribe(&room_name, address, true),
                    ChannelControl::VoiceUnsubscribe(address) => participant_audio_subscribe(&room_name, address, false),
                    ChannelControl::StreamerSubscribe => if let Err(err) = streamer_subscribe_channel(&room_name, true, true) {
                        error!("{err:?}");
                    },
                    ChannelControl::StreamerUnsubscribe => if let Err(err) = streamer_subscribe_channel(&room_name, false, false) {
                        error!("{err:?}");
                    },
                };
            }
        );
    }

    close_room(&room)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to close room: {:?}", e))?;

    Ok(())
}

// Define structures for the events coming from JavaScript
#[expect(dead_code, reason = "Some fields exist for consistency")]
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum RoomEvent {
    DataReceived {
        room_name: String,
        participant: Participant,
        payload: serde_bytes::ByteBuf,
    },
    TrackPublished {
        room_name: String,
        kind: String,
        participant: Participant,
    },
    TrackUnpublished {
        room_name: String,
        kind: String,
        participant: Participant,
    },
    TrackSubscribed {
        room_name: String,
        participant: Participant,
    },
    TrackUnsubscribed {
        room_name: String,
        participant: Participant,
    },
    ParticipantConnected {
        room_name: String,
        participant: Participant,
    },
    ParticipantDisconnected {
        room_name: String,
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

async fn handle_room_event(event: JsValue, transport_id: Entity, sender: Sender<NetworkUpdate>) {
    // Try to deserialize the event using serde_wasm_bindgen
    let event_result: Result<RoomEvent, _> = serde_wasm_bindgen::from_value(event);

    match event_result {
        Ok(room_event) => match room_event {
            RoomEvent::DataReceived {
                payload,
                participant,
                ..
            } => {
                if let Ok(packet) = rfc4::Packet::decode(payload.as_slice()) {
                    if let Some(message) = packet.message {
                        if let Some(address) = participant.identity.as_h160() {
                            let _ = sender
                                .send(
                                    PlayerUpdate {
                                        transport_id,
                                        message: PlayerMessage::PlayerData(message),
                                        address,
                                    }
                                    .into(),
                                )
                                .await;
                        } else {
                            let _ = sender
                                .send(
                                    NonPlayerUpdate {
                                        transport_id,
                                        address: participant.identity,
                                        message,
                                    }
                                    .into(),
                                )
                                .await;
                        }
                    }
                }
            }
            RoomEvent::TrackPublished {
                participant, kind, ..
            } => {
                debug!("pub {} {}", participant.identity, kind);
                if let Some(address) = participant.identity.as_h160() {
                    if kind == "audio" {
                        let _ = sender
                            .send(
                                PlayerUpdate {
                                    transport_id,
                                    message: PlayerMessage::AudioStreamAvailable {
                                        transport: transport_id,
                                    },
                                    address,
                                }
                                .into(),
                            )
                            .await;
                    }
                }
            }
            RoomEvent::TrackUnpublished {
                participant, kind, ..
            } => {
                debug!("unpub {} {}", participant.identity, kind);
                if let Some(address) = participant.identity.as_h160() {
                    if kind == "audio" {
                        let _ = sender
                            .send(
                                PlayerUpdate {
                                    transport_id,
                                    message: PlayerMessage::AudioStreamUnavailable {
                                        transport: transport_id,
                                    },
                                    address,
                                }
                                .into(),
                            )
                            .await;
                    }
                }
            }
            RoomEvent::TrackSubscribed { .. } => {
                debug!("Track subscribed event - audio is handled in JavaScript");
            }
            RoomEvent::TrackUnsubscribed { .. } => {
                debug!("Track unsubscribed event");
            }
            RoomEvent::ParticipantConnected { participant, .. } => {
                if let Some(address) = participant.identity.as_h160() {
                    if !participant.metadata.is_empty() {
                        let _ = sender
                            .send(
                                PlayerUpdate {
                                    transport_id,
                                    message: PlayerMessage::MetaData(participant.metadata),
                                    address,
                                }
                                .into(),
                            )
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

use crate::global_crdt::{ForeignAudioSource, ForeignPlayer};
use bevy::render::view::RenderLayers;
use common::{structs::AudioSettings, util::VolumePanning};

#[allow(clippy::type_complexity)]
pub fn locate_foreign_streams(
    mut streams: Query<(
        &GlobalTransform,
        Option<&RenderLayers>,
        &ForeignAudioSource,
        &ForeignPlayer,
    )>,
    pan: VolumePanning,
    settings: Res<AudioSettings>,
) {
    for (emitter_transform, render_layers, source, player) in streams.iter_mut() {
        if source.current_transport.is_some() {
            let (volume, panning) =
                pan.volume_and_panning(emitter_transform.translation(), render_layers);
            let volume = volume * settings.voice();

            update_participant_spatial_audio(
                &format!("{:#x}", player.address),
                -1.0 + 2.0 * panning,
                volume,
            );
        }
    }
}

fn participant_audio_subscribe(room_name: &str, address: H160, subscribe: bool) {
    if let Err(e) = subscribe_channel(room_name, &format!("{address:#x}"), subscribe) {
        warn!("Failed to (un)subscribe to {address:?}: {e:?}");
    } else {
        debug!("sub to {address:?}: {subscribe}");
    }
}
