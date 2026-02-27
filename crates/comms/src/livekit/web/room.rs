use bevy::prelude::*;
use js_sys::{Object, Reflect};
use tokio::sync::mpsc;
use wasm_bindgen::{convert::IntoWasmAbi, describe::WasmDescribe, prelude::*, JsValue};

use crate::livekit::web::{JsValueAbi, LocalParticipant, RoomEvent, RoomOptions, RoomResult};

#[wasm_bindgen(module = "/livekit_web_bindings.js")]
extern "C" {
    #[wasm_bindgen(catch)]
    async fn room_connect(
        url: &str,
        token: &str,
        room_options: JsValue,
        room_connect_options: JsValue,
        handler: &Closure<dyn Fn(RoomEvent)>,
    ) -> RoomResult<Room>;
    #[wasm_bindgen(catch)]
    async fn room_close(room: &Room) -> RoomResult<()>;
    #[wasm_bindgen]
    fn room_name(room: &Room) -> String;
    #[wasm_bindgen]
    fn room_local_participant(room: &Room) -> LocalParticipant;
}

#[derive(Debug, Clone)]
pub struct Room {
    inner: JsValue,
}

/// SAFETY: should be fine while WASM remains single threaded
unsafe impl Send for Room {}

/// SAFETY: should be fine while WASM remains single threaded
unsafe impl Sync for Room {}

impl Room {
    pub async fn connect(
        url: &str,
        token: &str,
        room_options: RoomOptions,
    ) -> RoomResult<(Room, mpsc::UnboundedReceiver<RoomEvent>)> {
        let url = url.to_owned();
        let token = token.to_owned();

        let room_connect_options = build_room_connect_options(&room_options);
        let room_options = build_room_options(&room_options);

        let (sender, receiver) = mpsc::unbounded_channel();
        let handler = Closure::new(move |room_event: RoomEvent| {
            if let Err(err) = sender.send(room_event) {
                error!("Failed to send room event due to '{err}'.");
            }
        });

        let room = room_connect(
            &url,
            &token,
            room_options.into(),
            room_connect_options.into(),
            &handler,
        )
        .await?;

        let _ = handler.into_js_value();

        Ok((room, receiver))
    }

    pub async fn close(&self) -> RoomResult<()> {
        room_close(self).await
    }

    pub fn name(&self) -> String {
        room_name(self)
    }

    pub fn local_participant(&self) -> LocalParticipant {
        room_local_participant(self)
    }
}

impl From<Room> for JsValue {
    fn from(value: Room) -> Self {
        value.inner
    }
}

impl AsRef<JsValue> for Room {
    fn as_ref(&self) -> &JsValue {
        &self.inner
    }
}

impl WasmDescribe for Room {
    fn describe() {
        JsValue::describe()
    }
}

impl IntoWasmAbi for &Room {
    type Abi = JsValueAbi;

    fn into_abi(self) -> JsValueAbi {
        self.inner.clone().into_abi()
    }
}

impl JsCast for Room {
    fn instanceof(value: &wasm_bindgen::JsValue) -> bool {
        error!("{value:?}");
        false
    }

    fn unchecked_from_js(value: wasm_bindgen::JsValue) -> Self {
        Room { inner: value }
    }

    fn unchecked_from_js_ref(value: &wasm_bindgen::JsValue) -> &Self {
        error!("{value:?}");
        panic!("todo");
    }
}

fn build_room_options(room_options: &RoomOptions) -> JsValue {
    let object = Object::new();
    Reflect::set(
        &object,
        &JsValue::from_str("adaptiveStream"),
        &JsValue::from_bool(room_options.adaptive_stream),
    )
    .unwrap();
    Reflect::set(
        &object,
        &JsValue::from_str("dynacast"),
        &JsValue::from_bool(room_options.dynacast),
    )
    .unwrap();
    Reflect::set(
        &object,
        &JsValue::from_str("stopLocalTrackOnUnpublish"),
        &JsValue::from_bool(true),
    )
    .unwrap();
    Reflect::set(
        &object,
        &JsValue::from_str("disconnectOnPageLeave"),
        &JsValue::from_bool(true),
    )
    .unwrap();
    Reflect::set(
        &object,
        &JsValue::from_str("webAudioMix"),
        &JsValue::from_bool(true),
    )
    .unwrap();
    object.into()
}

fn build_room_connect_options(room_options: &RoomOptions) -> JsValue {
    let object = Object::new();
    Reflect::set(
        &object,
        &JsValue::from_str("autoSubscribe"),
        &JsValue::from_bool(room_options.auto_subscribe),
    )
    .unwrap();
    object.into()
}
