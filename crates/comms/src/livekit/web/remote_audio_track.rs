use wasm_bindgen::{convert::IntoWasmAbi, describe::WasmDescribe, JsValue, prelude::wasm_bindgen};

use crate::livekit::web::JsValueAbi;
#[wasm_bindgen(module = "/livekit_web_bindings.js")]
extern "C" {
    #[wasm_bindgen]
    fn remote_audio_track_set_volume(
        remote_audio_track: &RemoteAudioTrack,
        volume: f32
    );
}

#[derive(Debug, Clone)]
pub struct RemoteAudioTrack {
    inner: JsValue,
}

impl RemoteAudioTrack {
    pub fn set_volume(&self, volume: f32) {
        remote_audio_track_set_volume(self, volume);
    }
}


impl From<JsValue> for RemoteAudioTrack {
    fn from(value: JsValue) -> Self {
        Self { inner: value }
    }
}

/// SAFETY: should be fine while WASM remains single-threaded
unsafe impl Send for RemoteAudioTrack {}

/// SAFETY: should be fine while WASM remains single-threaded
unsafe impl Sync for RemoteAudioTrack {}

impl WasmDescribe for RemoteAudioTrack {
    fn describe() {
        JsValue::describe();
    }
}

impl IntoWasmAbi for &RemoteAudioTrack {
    type Abi = JsValueAbi;

    fn into_abi(self) -> Self::Abi {
        self.inner.clone().into_abi()
    }
}
