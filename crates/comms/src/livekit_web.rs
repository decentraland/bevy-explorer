use std::sync::Arc;

use bevy::{prelude::*, utils::HashMap};
use http::Uri;
use prost::Message;
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
}

pub struct LivekitWebPlugin;

impl Plugin for LivekitWebPlugin {
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
    app_rx: Arc<Mutex<Receiver<NetworkMessage>>>,
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
    app_rx: Arc<Mutex<Receiver<NetworkMessage>>>,
    sender: Sender<PlayerUpdate>,
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
    let mut app_rx = app_rx.lock().await;
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

async fn handle_room_event(event: JsValue, transport_id: Entity, sender: Sender<PlayerUpdate>) {
    // Parse the event from JavaScript
    // This is a simplified version - you'll need to properly parse the JavaScript event object
    if let Ok(event_type) = js_sys::Reflect::get(&event, &JsValue::from_str("type")) {
        if let Some(event_type_str) = event_type.as_string() {
            match event_type_str.as_str() {
                "data_received" => {
                    if let Ok(payload) = js_sys::Reflect::get(&event, &JsValue::from_str("payload"))
                    {
                        if let Ok(participant) =
                            js_sys::Reflect::get(&event, &JsValue::from_str("participant"))
                        {
                            if let Ok(identity) =
                                js_sys::Reflect::get(&participant, &JsValue::from_str("identity"))
                            {
                                if let Some(identity_str) = identity.as_string() {
                                    if let Some(address) = identity_str.as_h160() {
                                        // Convert payload to bytes
                                        let data = js_sys::Uint8Array::new(&payload);
                                        let mut bytes = vec![0u8; data.length() as usize];
                                        data.copy_to(&mut bytes);

                                        if let Ok(packet) = rfc4::Packet::decode(bytes.as_slice()) {
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
                            }
                        }
                    }
                }
                "track_subscribed" => {
                    // Audio is now handled directly in JavaScript
                    // We just log this for debugging
                    debug!("Track subscribed event - audio is handled in JavaScript");
                }
                "track_unsubscribed" => {
                    // Track cleanup is handled in JavaScript
                    debug!("Track unsubscribed event");
                }
                "participant_connected" => {
                    if let Ok(participant) =
                        js_sys::Reflect::get(&event, &JsValue::from_str("participant"))
                    {
                        if let Ok(metadata) =
                            js_sys::Reflect::get(&participant, &JsValue::from_str("metadata"))
                        {
                            if let Ok(identity) =
                                js_sys::Reflect::get(&participant, &JsValue::from_str("identity"))
                            {
                                if let Some(metadata_str) = metadata.as_string() {
                                    if let Some(identity_str) = identity.as_string() {
                                        if let Some(address) = identity_str.as_h160() {
                                            if !metadata_str.is_empty() {
                                                let _ = sender
                                                    .send(PlayerUpdate {
                                                        transport_id,
                                                        message: PlayerMessage::MetaData(
                                                            metadata_str,
                                                        ),
                                                        address,
                                                    })
                                                    .await;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
