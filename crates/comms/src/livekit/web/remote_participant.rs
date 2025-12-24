use wasm_bindgen::JsValue;

use crate::livekit::web::{traits::GetFromJsValue, ParticipantIdentity, ParticipantSid};

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

impl GetFromJsValue for RemoteParticipant {
    fn get_from_js_value(js_value: &JsValue, key: &str) -> Option<Self> {
        js_sys::Reflect::get(&js_value, &JsValue::from(key))
            .ok()
            .map(|participant| RemoteParticipant { inner: participant })
    }
}
