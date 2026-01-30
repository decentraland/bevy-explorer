use bevy::prelude::*;
use serde::Deserialize;
use wasm_bindgen::{
    convert::{FromWasmAbi, IntoWasmAbi},
    describe::WasmDescribe,
    JsValue,
};

use crate::livekit::web::JsValueAbi;

#[derive(Debug, Clone, Deserialize)]
pub struct TrackSid(String);

impl WasmDescribe for TrackSid {
    fn describe() {
        JsValue::describe();
    }
}

impl FromWasmAbi for TrackSid {
    type Abi = JsValueAbi;

    unsafe fn from_abi(value: Self::Abi) -> Self {
        let js_value = JsValue::from_abi(value);
        let Some(sid) = js_value.as_string() else {
            panic!("Could not parse JsValue into Sid. From JsValue {js_value:?}.");
        };
        Self(sid)
    }
}

impl IntoWasmAbi for &TrackSid {
    type Abi = JsValueAbi;

    fn into_abi(self) -> Self::Abi {
        JsValue::from_str(&self.0).into_abi()
    }
}
