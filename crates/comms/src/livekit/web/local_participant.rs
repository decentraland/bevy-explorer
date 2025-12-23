use bevy::prelude::*;
use wasm_bindgen::{convert::FromWasmAbi, describe::WasmDescribe, JsValue};

use crate::livekit::web::{JsValueAbi, ParticipantIdentity, RoomResult};

#[derive(Debug, Clone)]
pub struct LocalParticipant {
    inner: JsValue,
}

/// SAFETY: should be fine while WASM remains single threaded
unsafe impl Send for LocalParticipant {}
/// SAFETY: should be fine while WASM remains single threaded
unsafe impl Sync for LocalParticipant {}

impl WasmDescribe for LocalParticipant {
    fn describe() {
        JsValue::describe();
    }
}

impl FromWasmAbi for LocalParticipant {
    type Abi = JsValueAbi;

    unsafe fn from_abi(value: JsValueAbi) -> Self {
        Self {
            inner: unsafe { JsValue::from_abi(value) },
        }
    }
}

impl LocalParticipant {
    pub async fn publish_data<T>(&self, data: T) -> RoomResult<()> {
        error!("todo publish_data");
        panic!("todo publish_data")
    }

    pub fn identity(&self) -> ParticipantIdentity {
        error!("todo identity");
        panic!("todo identity")
    }

    pub fn sid(&self) -> String {
        error!("todo sid");
        panic!("todo sid")
    }

    pub fn metadata(&self) -> String {
        error!("todo metadata");
        panic!("todo metadata")
    }
}
