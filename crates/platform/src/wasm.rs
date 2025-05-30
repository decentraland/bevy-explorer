use std::time::Duration;

use anyhow::anyhow;
use bevy::ecs::component::Component;
use common::structs::AppConfig;
use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
pub use tungstenite::client::IntoClientRequest;
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
pub fn write_config_file(_config: &AppConfig) {
    // do nothing
}

// dummy prepass markers for webgl
#[derive(Component)]
pub struct DepthPrepass;
#[derive(Component)]
pub struct NormalPrepass;
