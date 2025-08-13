use std::time::Duration;

use anyhow::anyhow;
use bevy::{ecs::component::Component, log::warn};
use common::structs::AppConfig;
use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
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
    let url = request.into_client_request()?.uri().to_string();
    let (_meta, stream) = ws_stream_wasm::WsMeta::connect(&url, None).await?;
    Ok(WebSocket { _meta, stream })
}

pub trait ReqwestBuilderExt {
    fn timeout(self, timeout: Duration) -> Self;
    fn connect_timeout(self, timeout: Duration) -> Self;
    fn use_native_tls(self) -> Self;
}

impl ReqwestBuilderExt for reqwest::ClientBuilder {
    fn timeout(self, _timeout: Duration) -> Self {
        self
    }

    fn connect_timeout(self, _timeout: Duration) -> Self {
        self
    }

    fn use_native_tls(self) -> Self {
        self
    }
}

impl ReqwestBuilderExt for reqwest::RequestBuilder {
    fn timeout(self, _timeout: Duration) -> Self {
        self
    }

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
pub fn write_config_file(config: &AppConfig) {
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

// dummy prepass markers for webgl
#[derive(Component)]
pub struct DepthPrepass;
#[derive(Component)]
pub struct NormalPrepass;

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
