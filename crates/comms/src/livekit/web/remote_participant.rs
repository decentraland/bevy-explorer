use wasm_bindgen::{
    convert::{FromWasmAbi, IntoWasmAbi},
    describe::WasmDescribe,
    prelude::wasm_bindgen,
    JsValue,
};

use crate::livekit::web::{
    traits::GetFromJsValue, JsValueAbi, ParticipantIdentity, ParticipantSid,
};

#[wasm_bindgen(module = "/livekit_web_bindings.js")]
extern "C" {
    #[wasm_bindgen]
    fn remote_participant_is_local(remote_participant: &RemoteParticipant) -> bool;
    #[wasm_bindgen]
    fn remote_participant_sid(remote_participant: &RemoteParticipant) -> String;
    #[wasm_bindgen]
    fn remote_participant_identity(remote_participant: &RemoteParticipant) -> String;
    #[wasm_bindgen]
    fn remote_participant_metadata(remote_participant: &RemoteParticipant) -> String;
}

#[derive(Debug, Clone)]
pub struct RemoteParticipant {
    pub inner: JsValue,
}

impl RemoteParticipant {
    pub fn is_local(&self) -> bool {
        // Should always be false
        remote_participant_is_local(self)
    }

    pub fn identity(&self) -> ParticipantIdentity {
        ParticipantIdentity(remote_participant_identity(self))
    }

    // pub fn name(&self) -> String {
    //     "".to_owned()
    // }

    pub fn metadata(&self) -> String {
        remote_participant_metadata(self)
    }

    pub fn sid(&self) -> ParticipantSid {
        ParticipantSid(remote_participant_sid(self))
    }
}

impl From<JsValue> for RemoteParticipant {
    fn from(value: JsValue) -> Self {
        Self { inner: value }
    }
}

/// SAFETY: should be fine while WASM remains single-threaded
unsafe impl Send for RemoteParticipant {}

/// SAFETY: should be fine while WASM remains single-threaded
unsafe impl Sync for RemoteParticipant {}

impl WasmDescribe for RemoteParticipant {
    fn describe() {
        JsValue::describe();
    }
}

impl FromWasmAbi for RemoteParticipant {
    type Abi = JsValueAbi;

    unsafe fn from_abi(value: JsValueAbi) -> Self {
        Self {
            inner: unsafe { JsValue::from_abi(value) },
        }
    }
}

impl IntoWasmAbi for &RemoteParticipant {
    type Abi = JsValueAbi;

    fn into_abi(self) -> JsValueAbi {
        self.inner.clone().into_abi()
    }
}

impl GetFromJsValue for RemoteParticipant {
    fn get_from_js_value(js_value: &JsValue, key: &str) -> Option<Self> {
        js_sys::Reflect::get(js_value, &JsValue::from(key))
            .ok()
            .filter(|participant| !(participant.is_null() || participant.is_undefined()))
            .map(|participant| RemoteParticipant { inner: participant })
    }
}
