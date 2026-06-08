use std::time::Duration;

use anyhow::anyhow;
use bevy::log::warn;
use bevy::{
    core_pipeline::{
        bloom::Bloom,
        prepass::{DepthPrepass, NormalPrepass},
        tonemapping::{DebandDither, Tonemapping},
    },
    ecs::bundle::Bundle,
    pbr::ShadowFilteringMethod,
    render::view::{ColorGrading, ColorGradingGlobal, ColorGradingSection},
};

use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use serde::Serialize;
pub use tungstenite::client::IntoClientRequest;
use wasm_bindgen_futures::spawn_local;
use ws_stream_wasm::{WsMessage, WsMeta, WsStream};
pub struct WebSocket {
    _meta: WsMeta,
    stream: WsStream,
}

impl WebSocket {
    pub async fn send(&mut self, item: tungstenite::Message) -> Result<(), anyhow::Error> {
        let message = match item {
            tungstenite::Message::Text(text) => ws_stream_wasm::WsMessage::Text(text),
            tungstenite::Message::Binary(bin) => ws_stream_wasm::WsMessage::Binary(bin),
            tungstenite::Message::Close(_) => {
                return self.stream.close().await.map_err(|e| anyhow!(e));
            }
            _ => return Err(anyhow!("unexpected message {item:?}")),
        };

        self.stream.send(message).await.map_err(|e| anyhow!(e))
    }

    pub async fn next(&mut self) -> Option<Result<tungstenite::Message, anyhow::Error>> {
        let next = self.stream.next().await;
        match next {
            Some(WsMessage::Text(text)) => Some(Ok(tungstenite::Message::Text(text))),
            Some(WsMessage::Binary(bin)) => Some(Ok(tungstenite::Message::Binary(bin))),
            None => None,
        }
    }

    pub fn split(self) -> (WebSocketSplitSink, WebSocketSplitStream) {
        let (sink, stream) = self.stream.split();
        (WebSocketSplitSink { sink }, WebSocketSplitStream { stream })
    }
}

pub struct WebSocketSplitSink {
    sink: SplitSink<WsStream, WsMessage>,
}

impl WebSocketSplitSink {
    pub async fn send(&mut self, item: tungstenite::Message) -> Result<(), anyhow::Error> {
        let message = match item {
            tungstenite::Message::Text(text) => ws_stream_wasm::WsMessage::Text(text),
            tungstenite::Message::Binary(bin) => ws_stream_wasm::WsMessage::Binary(bin),
            tungstenite::Message::Close(_) => {
                return self.sink.close().await.map_err(|e| anyhow!(e));
            }
            _ => return Err(anyhow!("unexpected message {item:?}")),
        };

        self.sink.send(message).await.map_err(|e| anyhow!(e))
    }
}

pub struct WebSocketSplitStream {
    stream: SplitStream<WsStream>,
}

impl WebSocketSplitStream {
    pub async fn next(&mut self) -> Option<Result<tungstenite::Message, anyhow::Error>> {
        let next = self.stream.next().await;
        match next {
            Some(WsMessage::Text(text)) => Some(Ok(tungstenite::Message::Text(text))),
            Some(WsMessage::Binary(bin)) => Some(Ok(tungstenite::Message::Binary(bin))),
            None => None,
        }
    }
}

pub async fn websocket<R>(request: R) -> Result<WebSocket, anyhow::Error>
where
    R: IntoClientRequest + Unpin,
{
    let request = request.into_client_request()?;
    let url = request.uri().to_string();
    let headers = request.headers();
    let protocol = headers.get("Sec-Websocket-Protocol");
    let (_meta, stream) = ws_stream_wasm::WsMeta::connect(
        &url,
        protocol
            .as_ref()
            .map(|protocol| vec![protocol.to_str().unwrap()]),
    )
    .await?;
    Ok(WebSocket { _meta, stream })
}

pub trait ReqwestBuilderExt {
    fn connect_timeout(self, timeout: Duration) -> Self;
    fn use_native_tls(self) -> Self;
}

impl ReqwestBuilderExt for reqwest::ClientBuilder {
    fn connect_timeout(self, _timeout: Duration) -> Self {
        self
    }

    fn use_native_tls(self) -> Self {
        self
    }
}

impl ReqwestBuilderExt for reqwest::RequestBuilder {
    fn connect_timeout(self, _timeout: Duration) -> Self {
        self
    }

    fn use_native_tls(self) -> Self {
        self
    }
}

pub fn compat<F>(f: F) -> F {
    f
}

pub fn project_directories() -> Option<directories::ProjectDirs> {
    None
}
pub fn write_config_file<T: Serialize + Clone + 'static>(config: &T) {
    use futures_lite::io::AsyncWriteExt;
    let config = config.clone();

    spawn_local(async move {
        let mut f = match web_fs::File::create("config.json").await {
            Ok(f) => f,
            Err(e) => {
                warn!("couldn't create config file: {e:?}");
                return;
            }
        };

        if let Err(e) = f
            .write_all(serde_json::to_string(&config).unwrap().as_bytes())
            .await
        {
            warn!("couldn't write config file: {e:?}");
        }
    })
}

#[derive(Default)]
pub struct AsyncRwLock<T>(spin::RwLock<T>);

impl<T> AsyncRwLock<T> {
    pub fn new(value: T) -> Self {
        Self(spin::RwLock::new(value))
    }

    pub async fn read(&self) -> spin::RwLockReadGuard<'_, T> {
        self.0.read()
    }

    pub async fn write(&self) -> spin::RwLockWriteGuard<'_, T> {
        self.0.write()
    }

    pub fn blocking_read(&self) -> spin::RwLockReadGuard<'_, T> {
        self.0.read()
    }

    pub fn blocking_write(&self) -> spin::RwLockWriteGuard<'_, T> {
        self.0.write()
    }

    pub fn try_read(&self) -> Result<spin::RwLockReadGuard<'_, T>, NoError> {
        Ok(self.0.read())
    }

    pub fn try_write(&self) -> Result<spin::RwLockWriteGuard<'_, T>, NoError> {
        Ok(self.0.write())
    }
}

#[derive(Debug)]
pub struct NoError;

pub fn platform_pointer_is_locked(_expected: bool) -> bool {
    web_sys::window()
        .and_then(|w| w.document())
        .map(|d| d.pointer_lock_element().is_some())
        .unwrap_or(false)
}

pub fn default_camera_components() -> impl Bundle {
    (
        Tonemapping::TonyMcMapface,
        DebandDither::Enabled,
        ColorGrading {
            global: ColorGradingGlobal {
                exposure: -0.5,
                ..Default::default()
            },
            shadows: ColorGradingSection {
                gamma: 0.75,
                ..Default::default()
            },
            midtones: ColorGradingSection {
                gamma: 0.75,
                ..Default::default()
            },
            highlights: ColorGradingSection {
                gamma: 0.75,
                ..Default::default()
            },
        },
        Bloom {
            intensity: 0.15,
            ..Bloom::OLD_SCHOOL
        },
        ShadowFilteringMethod::Gaussian,
        DepthPrepass,
        NormalPrepass,
    )
}

// Persist a scene's composite to the user's real filesystem via the File System Access API. The
// directory handle is acquired once with a picker and remembered in IndexedDB (keyed by scene id)
// so later saves skip the prompt. All of that lives in web_save.js; this just binds it. Returns
// the written path.
mod web_save {
    use wasm_bindgen::prelude::*;
    #[wasm_bindgen(module = "/src/web_save.js")]
    extern "C" {
        #[wasm_bindgen(catch, js_name = saveComposite)]
        pub async fn save_composite(
            key: &str,
            rel_path: &str,
            bytes: &[u8],
        ) -> Result<JsValue, JsValue>;
    }
}

pub async fn save_scene_composite(scene_hash: String, bytes: Vec<u8>) -> Result<String, String> {
    match web_save::save_composite(&scene_hash, "assets/scene/main.composite", &bytes).await {
        Ok(v) => Ok(v.as_string().unwrap_or_default()),
        Err(e) => Err(js_error_message(&e)),
    }
}

fn js_error_message(e: &wasm_bindgen::JsValue) -> String {
    e.as_string()
        .or_else(|| {
            js_sys::Reflect::get(e, &wasm_bindgen::JsValue::from_str("message"))
                .ok()
                .and_then(|m| m.as_string())
        })
        .unwrap_or_else(|| "save failed".to_string())
}
