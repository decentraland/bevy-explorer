use bevy::prelude::*;
use wasm_bindgen::{
    convert::{FromWasmAbi, IntoWasmAbi},
    describe::WasmDescribe,
    prelude::wasm_bindgen,
    JsCast, JsValue,
};

use crate::livekit::web::{AudioCaptureOptions, JsValueAbi, TrackSid};

#[wasm_bindgen(module = "/livekit_web_bindings.js")]
extern "C" {
    #[wasm_bindgen]
    async fn local_audio_track_new(options: AudioCaptureOptions) -> LocalAudioTrack;
    #[wasm_bindgen]
    fn local_audio_track_sid(local_audio_track: &LocalAudioTrack) -> TrackSid;
}

#[derive(Debug, Clone)]
pub struct LocalAudioTrack {
    pub inner: JsValue,
}

impl LocalAudioTrack {
    pub async fn new(options: AudioCaptureOptions) -> Self {
        local_audio_track_new(options).await
    }

    pub fn sid(&self) -> TrackSid {
        local_audio_track_sid(self)
    }
}

impl From<JsValue> for LocalAudioTrack {
    fn from(value: JsValue) -> Self {
        LocalAudioTrack { inner: value }
    }
}

impl From<LocalAudioTrack> for JsValue {
    fn from(value: LocalAudioTrack) -> Self {
        value.inner
    }
}

/// SAFETY: should be fine while WASM remains single-threaded
unsafe impl Send for LocalAudioTrack {}

/// SAFETY: should be fine while WASM remains single-threaded
unsafe impl Sync for LocalAudioTrack {}

impl WasmDescribe for LocalAudioTrack {
    fn describe() {
        JsValue::describe()
    }
}

impl FromWasmAbi for LocalAudioTrack {
    type Abi = JsValueAbi;

    unsafe fn from_abi(value: Self::Abi) -> Self {
        Self {
            inner: JsValue::from_abi(value),
        }
    }
}

impl IntoWasmAbi for &LocalAudioTrack {
    type Abi = JsValueAbi;

    fn into_abi(self) -> Self::Abi {
        self.inner.clone().into_abi()
    }
}

impl JsCast for LocalAudioTrack {
    fn instanceof(value: &JsValue) -> bool {
        panic!("{value:?}");
    }

    fn unchecked_from_js(value: JsValue) -> Self {
        Self { inner: value }
    }

    fn unchecked_from_js_ref(value: &JsValue) -> &Self {
        panic!("js_ref {:?}", value)
    }
}

impl AsRef<JsValue> for LocalAudioTrack {
    fn as_ref(&self) -> &JsValue {
        debug!("as_ref {:?}", self.inner);
        &self.inner
    }
}
