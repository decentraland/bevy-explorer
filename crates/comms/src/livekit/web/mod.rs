mod local_participant;
mod room;

use std::{
    error::Error,
    fmt::{Display, Formatter},
};

use bevy::{platform::sync::Arc, prelude::*};
use ethers_core::types::H160;
use serde::{Deserialize, Deserializer};
use wasm_bindgen::{
    convert::{FromWasmAbi, IntoWasmAbi, OptionFromWasmAbi},
    describe::WasmDescribe,
    prelude::*,
};

pub use {local_participant::LocalParticipant, room::Room};

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

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
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

// Define structures for the events coming from JavaScript
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RoomEvent {
    Connected,
    DataReceived {
        #[serde(deserialize_with = "data_received_payload_deserializer")]
        payload: Arc<Vec<u8>>,
        topic: Option<String>,
        kind: DataPacketKind,
        participant: RemoteParticipant,
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

    pub fn sid(&self) -> ParticipantSid {
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

    pub fn sid(&self) -> ParticipantSid {
        ParticipantSid("".to_owned())
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
        error!("todo sid");
        panic!("todo sid")
    }
}

#[derive(Clone)]
pub struct RemoteTrackPublication {
    abi: JsValueAbi,
}

impl RemoteTrackPublication {
    pub fn sid(&self) -> String {
        error!("todo sid");
        panic!("todo sid")
    }

    pub fn kind(&self) -> TrackKind {
        error!("todo kind");
        panic!("todo kind")
    }

    pub fn source(&self) -> TrackSource {
        error!("todo source");
        panic!("todo source")
    }

    pub fn track(&self) -> Option<RemoteTrack> {
        error!("todo track");
        panic!("todo track")
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deref)]
#[wasm_bindgen]
pub struct ParticipantSid(String);

impl std::fmt::Display for ParticipantSid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", self.0)
    }
}

impl From<JsValue> for RoomError {
    fn from(value: JsValue) -> Self {
        error!("{value:?}");
        serde_wasm_bindgen::from_value(value).expect("Room error")
    }
}


/// Kind of the packet.
/// 
/// Keep in track with
/// [https://github.com/livekit/protocol/blob/e7532dfc617d0c920eb905a93b6ca0d3ca4033e9/protobufs/livekit_models.proto#L324]
#[wasm_bindgen]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataPacketKind {
    Reliable = 0,
    Lossy = 1,
}

impl<'de> Deserialize<'de> for DataPacketKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let int = u32::deserialize(deserializer)?;
        let kind = match int {
            0 => DataPacketKind::Reliable,
            1 => DataPacketKind::Lossy,
            _ => unreachable!("Should always be 0 for Reliable or 1 for Lossy, but was {int}."),
        };
        Ok(kind)
    }
}

fn data_received_payload_deserializer<'de, D>(deserializer: D) -> Result<Arc<Vec<u8>>, D::Error>
where
    D: Deserializer<'de>,
{
    let buf = serde_bytes::ByteBuf::deserialize(deserializer)?;
    Ok(Arc::new(buf.into_vec()))
}
