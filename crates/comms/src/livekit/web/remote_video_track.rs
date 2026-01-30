use wasm_bindgen::{convert::IntoWasmAbi, describe::WasmDescribe, JsValue};

use crate::livekit::web::JsValueAbi;

#[derive(Debug, Clone)]
pub struct RemoteVideoTrack {
    inner: JsValue,
}

impl From<JsValue> for RemoteVideoTrack {
    fn from(value: JsValue) -> Self {
        Self { inner: value }
    }
}

/// SAFETY: should be fine while WASM remains single-threaded
unsafe impl Send for RemoteVideoTrack {}

/// SAFETY: should be fine while WASM remains single-threaded
unsafe impl Sync for RemoteVideoTrack {}

impl WasmDescribe for RemoteVideoTrack {
    fn describe() {
        JsValue::describe();
    }
}

impl IntoWasmAbi for &RemoteVideoTrack {
    type Abi = JsValueAbi;

    fn into_abi(self) -> Self::Abi {
        self.inner.clone().into_abi()
    }
}
