use bevy::{platform::sync::Arc, prelude::Deref};
use tokio::sync::mpsc;
use wasm_bindgen::{
    convert::{FromWasmAbi, IntoWasmAbi},
    prelude::wasm_bindgen,
    JsValue,
};

use crate::{
    livekit::web::{JsValueAbi, LocalParticipant, RoomEvent, RoomOptions, RoomResult},
    make_js_version,
};

#[wasm_bindgen(module = "/livekit_web_bindings.js")]
extern "C" {
    #[wasm_bindgen]
    fn room_name(room: &JsValue) -> String;
}

#[derive(Clone, Deref)]
pub struct Room {
    room: Arc<JsRoom>,
}
make_js_version!(JsRoom);

impl Room {
    pub async fn connect(
        address: &str,
        token: &str,
        room_options: RoomOptions,
    ) -> RoomResult<(Room, mpsc::UnboundedReceiver<RoomEvent>)> {
        todo!()
    }

    pub async fn close(&self) -> RoomResult<()> {
        todo!()
    }

    pub fn name(&self) -> String {
        let js_room = unsafe { JsValue::from_abi(self.room.abi) };
        let name = room_name(&js_room);
        js_room.into_abi();
        name
    }

    pub fn local_participant(&self) -> LocalParticipant {
        todo!()
    }
}
