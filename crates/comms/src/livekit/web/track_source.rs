use std::mem::ManuallyDrop;

use bevy::prelude::*;
use wasm_bindgen::{
    convert::{FromWasmAbi, IntoWasmAbi, OptionFromWasmAbi, OptionIntoWasmAbi},
    describe::WasmDescribe,
    JsValue,
};

use crate::livekit::web::JsValueAbi;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackSource {
    Unknown,
    Camera,
    Microphone,
    Screenshare,
    ScreenshareAudio,
}

impl WasmDescribe for TrackSource {
    fn describe() {
        JsValue::describe();
    }
}

impl FromWasmAbi for TrackSource {
    type Abi = JsValueAbi;

    unsafe fn from_abi(value: JsValueAbi) -> Self {
        let js_value = JsValue::from_abi(value);
        match js_value.as_string().as_deref() {
            Some("microphone") => Self::Microphone,
            Some("camera") => Self::Camera,
            Some("screen_share") => Self::Screenshare,
            Some("screen_share_audio") => Self::ScreenshareAudio,
            Some("unknown") => Self::Unknown,
            Some(other) => {
                error!("TrackSource was not a known source. Was '{other}'. Return Unknown.");
                Self::Unknown
            }
            None => {
                error!("TrackSource was not a string. Return Unknown.");
                Self::Unknown
            }
        }
    }
}

impl IntoWasmAbi for TrackSource {
    type Abi = JsValueAbi;

    fn into_abi(self) -> Self::Abi {
        match self {
            Self::Microphone => JsValue::from_str("microphone"),
            Self::Camera => JsValue::from_str("camera"),
            Self::Screenshare => JsValue::from_str("screen_share"),
            Self::ScreenshareAudio => JsValue::from_str("screen_share_audio"),
            Self::Unknown => JsValue::from_str("unknown"),
        }
        .into_abi()
    }
}

impl OptionFromWasmAbi for TrackSource {
    fn is_none(value: &JsValueAbi) -> bool {
        let js_value = ManuallyDrop::new(unsafe { JsValue::from_abi(*value) });
        js_value.is_null() || js_value.is_undefined()
    }
}

impl OptionIntoWasmAbi for TrackSource {
    fn none() -> <Self as IntoWasmAbi>::Abi {
        JsValue::null().into_abi()
    }
}
