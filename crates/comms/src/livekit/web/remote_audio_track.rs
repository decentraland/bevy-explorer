use wasm_bindgen::{convert::IntoWasmAbi, describe::WasmDescribe, JsValue};

use crate::livekit::web::JsValueAbi;

#[derive(Debug, Clone)]
pub struct RemoteAudioTrack {
    inner: JsValue,
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
