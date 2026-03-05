use std::mem::ManuallyDrop;

use wasm_bindgen::{
    convert::{FromWasmAbi, IntoWasmAbi, OptionFromWasmAbi},
    describe::WasmDescribe,
    prelude::wasm_bindgen,
    JsValue,
};

use crate::livekit::web::{GetFromJsValue, JsValueAbi, RemoteAudioTrack, RemoteVideoTrack};

#[wasm_bindgen(module = "/livekit_web_bindings.js")]
extern "C" {
    #[wasm_bindgen]
    fn remote_track_pan_and_volume(remote_track: &RemoteTrack, pan: f32, volume: f32);
}

#[derive(Clone, Debug)]
pub enum RemoteTrack {
    Audio(RemoteAudioTrack),
    Video(RemoteVideoTrack),
}

impl RemoteTrack {
    pub fn pan_and_volume(&self, pan: f32, volume: f32) {
        remote_track_pan_and_volume(self, pan, volume);
    }

    fn from_js_value(js_value: JsValue) -> Self {
        let kind = js_sys::Reflect::get(&js_value, &JsValue::from("kind"))
            .ok()
            .and_then(|kind| kind.as_string());
        match kind.as_deref() {
            Some("audio") => Self::Audio(RemoteAudioTrack::from(js_value)),
            Some("video") => Self::Video(RemoteVideoTrack::from(js_value)),
            Some(other) => {
                panic!("Unknown RemoteTrack kind {other}.")
            }
            None => {
                panic!("RemoteTrack did not have kind field.")
            }
        }
    }
}

impl WasmDescribe for RemoteTrack {
    fn describe() {
        JsValue::describe();
    }
}

impl FromWasmAbi for RemoteTrack {
    type Abi = JsValueAbi;

    unsafe fn from_abi(abi: Self::Abi) -> Self {
        let js_value = JsValue::from_abi(abi);
        Self::from_js_value(js_value)
    }
}

impl OptionFromWasmAbi for RemoteTrack {
    fn is_none(value: &<Self as FromWasmAbi>::Abi) -> bool {
        let js_value = ManuallyDrop::new(unsafe { JsValue::from_abi(*value) });
        js_value.is_null() || js_value.is_undefined()
    }
}

impl IntoWasmAbi for &RemoteTrack {
    type Abi = JsValueAbi;

    fn into_abi(self) -> Self::Abi {
        match self {
            RemoteTrack::Audio(audio) => audio.into_abi(),
            RemoteTrack::Video(video) => video.into_abi(),
        }
    }
}

impl GetFromJsValue for RemoteTrack {
    fn get_from_js_value(js_value: &JsValue, key: &str) -> Option<Self> {
        let track = js_sys::Reflect::get(js_value, &JsValue::from(key)).ok()?;
        Some(Self::from_js_value(track))
    }
}
