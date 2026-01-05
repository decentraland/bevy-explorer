use bevy::prelude::*;
use wasm_bindgen::{convert::IntoWasmAbi, describe::WasmDescribe, prelude::wasm_bindgen, JsValue};

use crate::livekit::web::{GetFromJsValue, JsValueAbi, LocalTrack, TrackKind, TrackSource};

#[wasm_bindgen(module = "/livekit_web_bindings.js")]
extern "C" {
    #[wasm_bindgen]
    fn local_track_publication_sid(local_track_publication: &LocalTrackPublication) -> String;
    #[wasm_bindgen]
    fn local_track_publication_kind(local_track_publication: &LocalTrackPublication) -> TrackKind;
    #[wasm_bindgen]
    fn local_track_publication_source(
        local_track_publication: &LocalTrackPublication,
    ) -> TrackSource;
}

#[derive(Debug, Clone)]
pub struct LocalTrackPublication {
    inner: JsValue,
}

impl LocalTrackPublication {
    pub fn sid(&self) -> String {
        local_track_publication_sid(self)
    }

    pub fn kind(&self) -> TrackKind {
        local_track_publication_kind(self)
    }

    pub fn source(&self) -> TrackSource {
        local_track_publication_source(self)
    }

    pub fn track(&self) -> Option<LocalTrack> {
        error!("todo track");
        panic!("todo track")
    }

    pub fn set_subscribed(&self, switch: bool) {
        todo!();
    }
}

impl From<JsValue> for LocalTrackPublication {
    fn from(value: JsValue) -> Self {
        LocalTrackPublication { inner: value }
    }
}

impl GetFromJsValue for LocalTrackPublication {
    fn get_from_js_value(js_value: &JsValue, key: &str) -> Option<Self> {
        js_sys::Reflect::get(js_value, &JsValue::from(key))
            .ok()
            .map(|publication| LocalTrackPublication { inner: publication })
    }
}

impl WasmDescribe for LocalTrackPublication {
    fn describe() {
        JsValue::describe()
    }
}

impl IntoWasmAbi for &LocalTrackPublication {
    type Abi = JsValueAbi;

    fn into_abi(self) -> JsValueAbi {
        self.inner.clone().into_abi()
    }
}

/// SAFETY: should be fine while WASM remains single threaded
unsafe impl Send for LocalTrackPublication {}

/// SAFETY: should be fine while WASM remains single threaded
unsafe impl Sync for LocalTrackPublication {}
