use bevy::prelude::*;
use tokio::sync::mpsc;
use wasm_bindgen::{convert::IntoWasmAbi, describe::WasmDescribe, prelude::*, JsValue};

use crate::livekit::web::{JsValueAbi, LocalParticipant, RoomEvent, RoomOptions, RoomResult};

#[wasm_bindgen(module = "/livekit_web_bindings.js")]
extern "C" {
    #[wasm_bindgen(catch)]
    async fn room_connect(
        url: &str,
        token: &str,
        room_options: InternalRoomOptions,
        room_connect_options: InternalRoomConnectOptions,
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

        let RoomOptions {
            auto_subscribe,
            adaptive_stream,
            dynacast,
            join_retries,
        } = room_options;

        let mut room_options = InternalRoomOptions::default();
        room_options.adaptiveStream = adaptive_stream;
        room_options.dynacast = dynacast;

        let mut room_connect_options = InternalRoomConnectOptions {
            auto_subscribe: auto_subscribe,
            max_retries: join_retries,
            ..Default::default()
        };

        let (sender, receiver) = mpsc::unbounded_channel();
        let handler = Closure::new(move |room_event: RoomEvent| {
            if let Err(err) = sender.send(room_event) {
                error!("Failed to send room event due to '{err}'.");
            }
        });

        let room = room_connect(&url, &token, room_options, room_connect_options, &handler).await?;

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

#[wasm_bindgen]
#[non_exhaustive]
#[expect(non_snake_case, reason = "Matching JS names")]
struct InternalRoomOptions {
    pub adaptiveStream: bool,
    pub dynacast: bool,
    pub stopLocalTrackOnUnpublish: bool,
    pub disconnectOnPageLeave: bool,
}

impl Default for InternalRoomOptions {
    fn default() -> Self {
        Self {
            adaptiveStream: false,
            dynacast: false,
            stopLocalTrackOnUnpublish: true,
            disconnectOnPageLeave: true,
        }
    }
}

#[wasm_bindgen]
#[non_exhaustive]
struct InternalRoomConnectOptions {
    #[wasm_bindgen(js_name = "autoSubscribe")]
    pub auto_subscribe: bool,
    #[wasm_bindgen(js_name = "peerConnectionTimeout")]
    pub peer_connection_timeout: u32,
    #[wasm_bindgen(js_name = "maxRetries")]
    pub max_retries: u32,
    #[wasm_bindgen(js_name = "websocketTimeout")]
    pub websocket_timeout: u32,
}

impl Default for InternalRoomConnectOptions {
    fn default() -> Self {
        Self {
            auto_subscribe: true,
            peer_connection_timeout: 15000,
            max_retries: 1,
            websocket_timeout: 15000,
        }
    }
}
