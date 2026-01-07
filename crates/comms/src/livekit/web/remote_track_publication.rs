use bevy::prelude::*;
use wasm_bindgen::{convert::IntoWasmAbi, describe::WasmDescribe, prelude::wasm_bindgen, JsValue};

use crate::livekit::web::{GetFromJsValue, JsValueAbi, TrackKind, TrackSource};

#[wasm_bindgen(module = "/livekit_web_bindings.js")]
extern "C" {
    #[wasm_bindgen]
    fn remote_track_publication_sid(remote_track_publication: &RemoteTrackPublication) -> String;
    #[wasm_bindgen]
    fn remote_track_publication_kind(
        remote_track_publication: &RemoteTrackPublication,
    ) -> TrackKind;
    #[wasm_bindgen]
    fn remote_track_publication_source(
        remote_track_publication: &RemoteTrackPublication,
    ) -> TrackSource;
    #[wasm_bindgen]
    fn remote_track_publication_set_subscribed(
        remote_track_publication: &RemoteTrackPublication,
        subscribed: bool,
    );
}

#[derive(Debug, Clone)]
pub struct RemoteTrackPublication {
    inner: JsValue,
}

impl RemoteTrackPublication {
    pub fn sid(&self) -> String {
        remote_track_publication_sid(self)
    }

    pub fn kind(&self) -> TrackKind {
        remote_track_publication_kind(self)
    }

    pub fn source(&self) -> TrackSource {
        remote_track_publication_source(self)
    }

    pub fn set_subscribed(&self, subscribed: bool) {
        remote_track_publication_set_subscribed(self, subscribed)
    }
}

impl From<JsValue> for RemoteTrackPublication {
    fn from(value: JsValue) -> Self {
        RemoteTrackPublication { inner: value }
    }
}

impl GetFromJsValue for RemoteTrackPublication {
    fn get_from_js_value(js_value: &JsValue, key: &str) -> Option<Self> {
        js_sys::Reflect::get(js_value, &JsValue::from(key))
            .ok()
            .map(|publication| RemoteTrackPublication { inner: publication })
    }
}

impl WasmDescribe for RemoteTrackPublication {
    fn describe() {
        JsValue::describe()
    }
}

impl IntoWasmAbi for &RemoteTrackPublication {
    type Abi = JsValueAbi;

    fn into_abi(self) -> JsValueAbi {
        self.inner.clone().into_abi()
    }
}

/// SAFETY: should be fine while WASM remains single threaded
unsafe impl Send for RemoteTrackPublication {}

/// SAFETY: should be fine while WASM remains single threaded
unsafe impl Sync for RemoteTrackPublication {}
