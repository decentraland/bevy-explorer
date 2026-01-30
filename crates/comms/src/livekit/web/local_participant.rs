use bevy::prelude::*;
use wasm_bindgen::{
    convert::{FromWasmAbi, IntoWasmAbi},
    describe::WasmDescribe,
    prelude::*,
    JsValue,
};

use crate::livekit::web::{
    DataPacket, JsValueAbi, LocalTrack, LocalTrackPublication, ParticipantIdentity, ParticipantSid,
    RoomResult, TrackPublishOptions,
};

#[wasm_bindgen(module = "/livekit_web_bindings.js")]
extern "C" {
    #[wasm_bindgen(catch)]
    async fn local_participant_publish_data(
        local_participant: &LocalParticipant,
        data: &[u8],
        data_publish_options: DataPublishOptions,
    ) -> RoomResult<()>;
    #[wasm_bindgen(catch)]
    async fn local_participant_publish_track(
        local_participant: &LocalParticipant,
        local_track: &LocalTrack,
        track_publish_options: TrackPublishOptions,
    ) -> RoomResult<LocalTrackPublication>;
    #[wasm_bindgen(catch)]
    async fn local_participant_unpublish_track(
        local_participant: &LocalParticipant,
        local_track: &LocalTrack,
    ) -> RoomResult<LocalTrackPublication>;
    #[wasm_bindgen]
    fn local_participant_is_local(local_participant: &LocalParticipant) -> bool;
    #[wasm_bindgen]
    fn local_participant_sid(local_participant: &LocalParticipant) -> String;
    #[wasm_bindgen]
    fn local_participant_identity(local_participant: &LocalParticipant) -> String;
    #[wasm_bindgen]
    fn local_participant_metadata(local_participant: &LocalParticipant) -> String;
}

#[derive(Debug, Clone)]
pub struct LocalParticipant {
    inner: JsValue,
}

impl LocalParticipant {
    pub async fn publish_data(&self, data: DataPacket) -> RoomResult<()> {
        let DataPacket {
            payload,
            reliable,
            topic,
            destination_identities,
        } = data;

        let data_publish_options = DataPublishOptions {
            reliable,
            topic,
            destination_identities: destination_identities
                .into_iter()
                .map(|participant_identity| participant_identity.0)
                .collect(),
        };

        local_participant_publish_data(self, &payload, data_publish_options).await
    }

    pub async fn publish_track(
        &self,
        track: LocalTrack,
        options: TrackPublishOptions,
    ) -> RoomResult<LocalTrackPublication> {
        local_participant_publish_track(self, &track, options).await
    }

    pub async fn unpublish_track(
        &self,
        local_track: &LocalTrack,
    ) -> RoomResult<LocalTrackPublication> {
        local_participant_unpublish_track(self, local_track).await
    }

    pub fn is_local(&self) -> bool {
        // Should always be true
        local_participant_is_local(self)
    }

    pub fn identity(&self) -> ParticipantIdentity {
        ParticipantIdentity(local_participant_identity(self))
    }

    pub fn sid(&self) -> ParticipantSid {
        ParticipantSid(local_participant_sid(self))
    }

    pub fn metadata(&self) -> String {
        local_participant_metadata(self)
    }
}

impl From<JsValue> for LocalParticipant {
    fn from(value: JsValue) -> Self {
        Self { inner: value }
    }
}

/// SAFETY: should be fine while WASM remains single threaded
unsafe impl Send for LocalParticipant {}
/// SAFETY: should be fine while WASM remains single threaded
unsafe impl Sync for LocalParticipant {}

impl WasmDescribe for LocalParticipant {
    fn describe() {
        JsValue::describe();
    }
}

impl FromWasmAbi for LocalParticipant {
    type Abi = JsValueAbi;

    unsafe fn from_abi(value: JsValueAbi) -> Self {
        Self {
            inner: unsafe { JsValue::from_abi(value) },
        }
    }
}

impl IntoWasmAbi for &LocalParticipant {
    type Abi = JsValueAbi;

    fn into_abi(self) -> JsValueAbi {
        self.inner.clone().into_abi()
    }
}

#[expect(dead_code, reason = "Read on JavaScript side")]
#[wasm_bindgen]
struct DataPublishOptions {
    #[wasm_bindgen = "reliable"]
    reliable: bool,
    #[wasm_bindgen = "topic"]
    topic: Option<String>,
    #[wasm_bindgen = "destinationIdentities"]
    destination_identities: Vec<String>,
}
