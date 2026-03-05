use wasm_bindgen::{
    convert::{FromWasmAbi, IntoWasmAbi},
    describe::WasmDescribe,
    prelude::wasm_bindgen,
    JsValue,
};

use crate::livekit::web::{
    GetFromJsValue, JsValueAbi, LocalParticipant, ParticipantIdentity, ParticipantSid,
    RemoteParticipant,
};

#[wasm_bindgen(module = "/livekit_web_bindings.js")]
extern "C" {
    #[wasm_bindgen]
    fn participant_is_local(participant: &Participant) -> bool;
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

impl WasmDescribe for Participant {
    fn describe() {
        JsValue::describe();
    }
}

impl FromWasmAbi for Participant {
    type Abi = JsValueAbi;

    unsafe fn from_abi(abi: JsValueAbi) -> Self {
        let js_value = JsValue::from_abi(abi);
        let participant = RemoteParticipant::from(js_value.clone());
        if participant.is_local() {
            Participant::Local(LocalParticipant::from(js_value))
        } else {
            Participant::Remote(participant)
        }
    }
}

impl IntoWasmAbi for &Participant {
    type Abi = JsValueAbi;

    fn into_abi(self) -> Self::Abi {
        match self {
            Participant::Local(local) => local.into_abi(),
            Participant::Remote(remote) => remote.into_abi(),
        }
    }
}

impl GetFromJsValue for Participant {
    fn get_from_js_value(js_value: &JsValue, key: &str) -> Option<Self> {
        let js_value = js_sys::Reflect::get(js_value, &JsValue::from(key)).ok()?;
        let participant = RemoteParticipant::from(js_value.clone());
        if participant.is_local() {
            Some(Participant::Local(LocalParticipant::from(js_value)))
        } else {
            Some(Participant::Remote(participant))
        }
    }
}
