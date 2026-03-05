use wasm_bindgen::{convert::IntoWasmAbi, describe::WasmDescribe, JsValue};

use crate::livekit::web::{JsValueAbi, LocalAudioTrack, LocalVideoTrack};

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
