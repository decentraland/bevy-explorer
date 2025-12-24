use wasm_bindgen::JsValue;

use crate::livekit::web::{ParticipantIdentity, ParticipantSid};

#[derive(Debug, Clone)]
pub struct RemoteParticipant {
    pub inner: JsValue,
}

impl RemoteParticipant {
    pub fn identity(&self) -> ParticipantIdentity {
        ParticipantIdentity("".to_owned())
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

/// SAFETY: should be fine while WASM remains single-threaded
unsafe impl Send for RemoteParticipant {}

/// SAFETY: should be fine while WASM remains single-threaded
unsafe impl Sync for RemoteParticipant {}
