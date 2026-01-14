mod local_audio_track;
mod local_participant;
mod local_track;
mod local_track_publication;
mod participant;
mod remote_audio_track;
mod remote_participant;
mod remote_track;
mod remote_track_publication;
mod remote_video_track;
mod room;
mod room_event;
mod track_sid;
mod track_source;
mod traits;

use std::{
    error::Error,
    fmt::{Display, Formatter},
};

use bevy::prelude::*;
use js_sys::{Object, Reflect};
use serde::{Deserialize, Deserializer};
use wasm_bindgen::{
    convert::{FromWasmAbi, IntoWasmAbi},
    describe::WasmDescribe,
    prelude::*,
};

use crate::livekit::web::traits::GetFromJsValue;
pub use crate::livekit::web::{
    local_audio_track::LocalAudioTrack, local_participant::LocalParticipant,
    local_track::LocalTrack, local_track_publication::LocalTrackPublication,
    participant::Participant, remote_audio_track::RemoteAudioTrack,
    remote_participant::RemoteParticipant, remote_track::RemoteTrack,
    remote_track_publication::RemoteTrackPublication, remote_video_track::RemoteVideoTrack,
    room::Room, room_event::RoomEvent, track_sid::TrackSid, track_source::TrackSource,
};

type JsValueAbi = <JsValue as IntoWasmAbi>::Abi;

pub type RoomResult<T> = Result<T, RoomError>;

#[derive(Debug, Default, Clone)]
pub struct RoomOptions {
    pub auto_subscribe: bool,
    pub adaptive_stream: bool,
    pub dynacast: bool,
    // pub e2ee: Option<E2eeOptions>,
    // pub rtc_config: RtcConfiguration,
    pub join_retries: u32,
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
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackKind {
    Audio,
    Video,
}

impl WasmDescribe for TrackKind {
    fn describe() {
        JsValue::describe();
    }
}

impl FromWasmAbi for TrackKind {
    type Abi = JsValueAbi;

    unsafe fn from_abi(value: JsValueAbi) -> Self {
        let js_value = JsValue::from_abi(value);
        match js_value.as_string().as_deref() {
            Some("audio") => Self::Audio,
            Some("video") => Self::Video,
            Some(other) => {
                error!("TrackKind was not a known kind. Was '{other}'. Assuming Audio.");
                Self::Audio
            }
            None => {
                error!("TrackKind was not a string. Assuming Audio.");
                Self::Audio
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deref)]
#[wasm_bindgen]
pub struct ParticipantSid(String);

impl std::fmt::Display for ParticipantSid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
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

impl GetFromJsValue for DataPacketKind {
    fn get_from_js_value(js_value: &JsValue, key: &str) -> Option<Self> {
        js_sys::Reflect::get(js_value, &JsValue::from(key))
            .ok()
            .and_then(|kind| serde_wasm_bindgen::from_value::<DataPacketKind>(kind).ok())
    }
}

#[derive(Debug)]
pub enum ConnectionState {
    Connected,
    Reconnecting,
    Disconnected,
}

#[derive(Debug, Clone)]
pub struct LocalVideoTrack {
    inner: JsValue,
}

/// SAFETY: should be fine while WASM remains single-threaded
unsafe impl Send for LocalVideoTrack {}

/// SAFETY: should be fine while WASM remains single-threaded
unsafe impl Sync for LocalVideoTrack {}

#[derive(Debug)]
pub struct TrackPublishOptions {
    // pub video_encoding: Option<VideoEncoding>,
    // pub audio_encoding: Option<AudioEncoding>,
    // pub video_codec: VideoCodec,
    // pub dtx: bool,
    // pub red: bool,
    pub simulcast: Option<bool>,
    pub source: TrackSource,
    pub stream: Option<String>,
    pub preconnect_buffer: Option<bool>,
}

impl Default for TrackPublishOptions {
    fn default() -> Self {
        Self {
            // video_encoding: None,
            // audio_encoding: None,
            // video_codec: VideoCodec::VP8,
            // dtx: true,
            // red: true,
            simulcast: Some(true),
            source: TrackSource::Unknown,
            stream: Some("".to_string()),
            preconnect_buffer: Some(false),
        }
    }
}

impl WasmDescribe for TrackPublishOptions {
    fn describe() {
        JsValue::describe()
    }
}

impl IntoWasmAbi for TrackPublishOptions {
    type Abi = JsValueAbi;

    fn into_abi(self) -> Self::Abi {
        let object = Object::new();

        if let Some(simulcast) = self.simulcast {
            let _ = Reflect::set(&object, &"simulcast".into(), &JsValue::from(simulcast));
        }

        JsValue::from(object).into_abi()
    }
}

#[wasm_bindgen]
#[derive(Debug, Default)]
pub struct AudioCaptureOptions {
    #[wasm_bindgen(js_name = "autoGainControl")]
    pub auto_gain_control: Option<bool>,
    #[wasm_bindgen(js_name = "channelCount")]
    pub channel_count: Option<u64>,
    #[wasm_bindgen(js_name = "echoCancellation")]
    pub echo_cancellation: Option<bool>,
    pub latency: Option<f64>,
    #[wasm_bindgen(js_name = "noiseSuppresion")]
    pub noise_suppression: Option<bool>,
    #[wasm_bindgen(js_name = "voiceIsolation")]
    pub voice_isolation: Option<bool>,
    #[wasm_bindgen(js_name = "sampleRate")]
    pub sample_rate: Option<u64>,
    #[wasm_bindgen(js_name = "sampleSize")]
    pub sample_size: Option<u64>,
}
