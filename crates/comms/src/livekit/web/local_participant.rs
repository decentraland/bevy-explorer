use bevy::prelude::*;
use wasm_bindgen::{
    convert::{FromWasmAbi, IntoWasmAbi},
    describe::WasmDescribe,
    prelude::*,
    JsValue,
};

use crate::livekit::web::{JsValueAbi, ParticipantIdentity, ParticipantSid, RoomResult};

#[wasm_bindgen(module = "/livekit_web_bindings.js")]
extern "C" {
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
    pub async fn publish_data<T>(&self, data: T) -> RoomResult<()> {
        error!("todo publish_data");
        panic!("todo publish_data")
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

