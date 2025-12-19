mod room;

use std::{
    error::Error,
    fmt::{Display, Formatter},
};

use bevy::{platform::sync::Arc, prelude::*};
use ethers_core::types::H160;
use serde::Deserialize;
use tokio::sync::mpsc::{Receiver, Sender};
use wasm_bindgen::{
    convert::{FromWasmAbi, IntoWasmAbi, OptionFromWasmAbi},
    prelude::*,
};
use wasm_bindgen_futures::spawn_local;

use crate::{
    global_crdt::{ChannelControl, GlobalCrdtState, PlayerUpdate},
    livekit::{room::LivekitRoom, LivekitConnection, LivekitTransport},
    NetworkMessage, NetworkMessageRecipient,
};

pub use room::Room;

#[wasm_bindgen(module = "/livekit_web_bindings.js")]
extern "C" {
    #[wasm_bindgen(catch)]
    pub async fn connect_room(url: &str, token: &str) -> Result<JsValue, JsValue>;

    #[wasm_bindgen]
    pub fn get_room(room_name: &str) -> JsValue;

    #[wasm_bindgen]
    pub fn recv_room_event(room: &JsValue) -> Option<RoomEvent>;

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
    pub async fn close_room(room: &JsValue) -> Result<(), JsValue>;

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
    pub fn set_microphone_enabled(enabled: bool) -> Result<(), JsValue>;

    #[wasm_bindgen(catch)]
    pub fn is_microphone_available() -> Result<bool, JsValue>;

    #[wasm_bindgen(catch)]
    pub fn set_participant_spatial_audio(
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
    pub fn streamer_subscribe_channel(
        room_name: &str,
        subscribe_audio: bool,
        subscribe_video: bool,
    ) -> Result<(), JsValue>;
}

type JsValueAbi = <JsValue as IntoWasmAbi>::Abi;

#[macro_export]
macro_rules! make_js_version {
    ($name:ident) => {
        struct $name {
            abi: JsValueAbi,
        }

        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let js = unsafe { JsValue::from_abi(self.abi) };
                let res = writeln!(f, "{js:?}");
                js.into_abi();
                res
            }
        }

        impl Drop for $name {
            fn drop(&mut self) {
                let _ = unsafe { JsValue::from_abi(self.abi) };
            }
        }
    };
}

pub type RoomResult<T> = Result<T, RoomError>;

#[derive(Debug, Default, Clone)]
pub struct RoomOptions {
    pub auto_subscribe: bool,
    pub adaptive_stream: bool,
    pub dynacast: bool,
    // pub e2ee: Option<E2eeOptions>,
    // pub rtc_config: RtcConfiguration,
    // pub join_retries: u32,
}

#[derive(Debug)]
pub enum RoomError {
    Other(String),
}

impl Display for RoomError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{self:?}")?;
        Ok(())
    }
}

impl Error for RoomError {}

#[allow(clippy::type_complexity)]
pub fn connect_livekit(
    mut commands: Commands,
    mut new_livekits: Query<
        (Entity, &mut LivekitTransport, &LivekitRoom),
        Without<LivekitConnection>,
    >,
    player_state: Res<GlobalCrdtState>,
) {
    for (transport_id, mut new_transport, livekit_room) in new_livekits.iter_mut() {
        debug!("spawn lk connect");
        let receiver = new_transport.receiver.take().unwrap();
        let sender = player_state.get_sender();

        // For WASM, we directly call the handler which will spawn the async task
        if let Err(e) = livekit_handler_inner(livekit_room.name(), receiver, sender) {
            warn!("Failed to start livekit connection: {e}");
        }

        commands.entity(transport_id).try_insert(LivekitConnection);
    }
}

fn livekit_handler_inner(
    room_name: String,
    app_rx: Receiver<NetworkMessage>,
    sender: Sender<PlayerUpdate>,
) -> Result<(), anyhow::Error> {
    // In WASM, we can't block or create threads, so we just spawn the async task
    spawn_local(async move {
        if let Err(e) = run_livekit_session(room_name, app_rx, sender).await {
            error!("LiveKit session error: {:?}", e);
        }
    });

    Ok(())
}

async fn run_livekit_session(
    room_name: String,
    mut app_rx: Receiver<NetworkMessage>,
    sender: Sender<PlayerUpdate>,
) -> Result<(), anyhow::Error> {
    loop {
        // Check if sender is closed (indicates we should stop)
        if sender.is_closed() {
            debug!("Sender closed, stopping LiveKit connection attempts");
            break;
        }

        match connect_and_handle_session(room_name.clone(), &mut app_rx).await {
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
    room_name: String,
    app_rx: &mut Receiver<NetworkMessage>,
) -> Result<(), anyhow::Error> {
    let room = get_room(&room_name);

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
                    NetworkMessageRecipient::Peer(address) => js_sys::Array::of1(&JsValue::from_str(&format!("{:#x}", address))),
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
        );
    }

    room.into_abi();

    Ok(())
}

// Define structures for the events coming from JavaScript
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RoomEvent {
    DataReceived {
        room_name: String,
        participant: RemoteParticipant,
        payload: serde_bytes::ByteBuf,
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
        serde_wasm_bindgen::from_value(JsValue::from_abi(abi)).unwrap()
    }
}

impl OptionFromWasmAbi for RoomEvent {
    fn is_none(abi: &Self::Abi) -> bool {
        [0, JsValue::NULL.into_abi(), JsValue::UNDEFINED.into_abi()].contains(abi)
    }
}

#[derive(Debug, Clone)]
pub enum Participant {
    Local(LocalParticipant),
    Remote(RemoteParticipant),
}

impl Participant {
    pub fn identity(&self) -> ParticipantIdentity {
        match self {
            Self::Local(l) => l.identity(),
            Self::Remote(r) => r.identity(),
        }
    }

    pub fn sid(&self) -> String {
        match self {
            Self::Local(l) => l.sid(),
            Self::Remote(r) => r.sid(),
        }
    }

    pub fn metadata(&self) -> String {
        match self {
            Self::Local(l) => l.metadata(),
            Self::Remote(r) => r.metadata(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteParticipant {
    pub identity: String,
    #[serde(default)]
    pub metadata: String,
}

impl RemoteParticipant {
    pub fn identity(&self) -> ParticipantIdentity {
        ParticipantIdentity(self.identity.clone())
    }

    pub fn name(&self) -> String {
        "".to_owned()
    }

    pub fn metadata(&self) -> String {
        "".to_owned()
    }

    pub fn sid(&self) -> String {
        "".to_owned()
    }
}

#[derive(Debug, Clone)]
pub struct LocalParticipant {
    participant: Arc<JsLocalParticipant>,
}
make_js_version!(JsLocalParticipant);

impl LocalParticipant {
    pub async fn publish_data<T>(&self, data: T) -> RoomResult<()> {
        todo!()
    }

    pub fn identity(&self) -> ParticipantIdentity {
        todo!()
    }

    pub fn sid(&self) -> String {
        todo!()
    }

    pub fn metadata(&self) -> String {
        todo!()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TrackSid;

#[derive(Debug, Clone)]
pub struct DataPacket {
    pub payload: Vec<u8>,
    pub topic: Option<String>,
    pub reliable: bool,
    pub destination_identities: Vec<ParticipantIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deref)]
pub struct ParticipantIdentity(pub String);

impl std::fmt::Display for ParticipantIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", self.0)
    }
}

#[derive(Clone, Debug)]
pub enum RemoteTrack {
    Audio(RemoteAudioTrack),
    Video(RemoteVideoTrack),
}

impl RemoteTrack {
    pub fn sid(&self) -> String {
        todo!()
    }
}

#[derive(Clone)]
pub struct RemoteTrackPublication {
    abi: JsValueAbi,
}

impl RemoteTrackPublication {
    pub fn sid(&self) -> String {
        todo!()
    }

    pub fn kind(&self) -> TrackKind {
        todo!()
    }

    pub fn source(&self) -> TrackSource {
        todo!()
    }

    pub fn track(&self) -> Option<RemoteTrack> {
        todo!()
    }

    pub fn set_subscribed(&self, switch: bool) {}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackKind {
    Audio,
    Video,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackSource {
    Unknown,
    Camera,
    Microphone,
    Screenshare,
    ScreenshareAudio,
}

#[derive(Debug, Clone)]
pub struct RemoteAudioTrack {
    abi: JsValueAbi,
}

#[derive(Debug, Clone)]
pub struct RemoteVideoTrack {
    abi: JsValueAbi,
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

pub fn participant_audio_subscribe(room_name: &str, address: H160, subscribe: bool) {
    if let Err(e) = subscribe_channel(room_name, &format!("{address:#x}"), subscribe) {
        warn!("Failed to (un)subscribe to {address:?}: {e:?}");
    } else {
        debug!("sub to {address:?}: {subscribe}");
    }
}
