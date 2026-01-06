use wasm_bindgen::{describe::WasmDescribe, JsValue, convert::IntoWasmAbi};

use crate::livekit::web::{LocalAudioTrack, LocalVideoTrack, JsValueAbi};

pub enum LocalTrack {
    Audio(LocalAudioTrack),
    Video(LocalVideoTrack),
}

impl WasmDescribe for LocalTrack {
    fn describe() {
        JsValue::describe();
    }
}

impl IntoWasmAbi for &LocalTrack {
    type Abi = JsValueAbi;

    fn into_abi(self) -> Self::Abi {
        match self {
            LocalTrack::Audio(audio) => audio.inner.clone().into_abi(),
            LocalTrack::Video(video) => video.inner.clone().into_abi(),
        }
    }
}