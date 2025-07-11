use async_tungstenite::{async_std::ConnectStream, WebSocketStream};
use common::structs::AppConfig;
use futures_util::{
    sink::SinkExt,
    stream::{SplitSink, SplitStream},
    StreamExt,
};
pub use tungstenite::client::IntoClientRequest;

pub struct WebSocket {
    inner: WebSocketStream<ConnectStream>,
}

impl WebSocket {
    pub async fn send(&mut self, item: tungstenite::Message) -> Result<(), anyhow::Error> {
        self.inner.send(item).await.map_err(|e| anyhow::anyhow!(e))
    }

    pub async fn next(&mut self) -> Option<Result<tungstenite::Message, anyhow::Error>> {
        self.inner
            .next()
            .await
            .map(|result| result.map_err(|e| anyhow::anyhow!(e)))
    }

    pub fn split(self) -> (WebSocketSplitSink, WebSocketSplitStream) {
        let (w, r) = self.inner.split();
        (
            WebSocketSplitSink { inner: w },
            WebSocketSplitStream { inner: r },
        )
    }
}

pub struct WebSocketSplitSink {
    inner: SplitSink<WebSocketStream<ConnectStream>, tungstenite::Message>,
}

impl WebSocketSplitSink {
    pub async fn send(&mut self, item: tungstenite::Message) -> Result<(), anyhow::Error> {
        self.inner.send(item).await.map_err(|e| anyhow::anyhow!(e))
    }
}

pub struct WebSocketSplitStream {
    inner: SplitStream<WebSocketStream<ConnectStream>>,
}

impl WebSocketSplitStream {
    pub async fn next(&mut self) -> Option<Result<tungstenite::Message, anyhow::Error>> {
        self.inner
            .next()
            .await
            .map(|result| result.map_err(|e| anyhow::anyhow!(e)))
    }
}

pub async fn websocket<R>(request: R) -> Result<WebSocket, anyhow::Error>
where
    R: IntoClientRequest + Unpin,
{
    let (stream, _response) = async_tungstenite::async_std::connect_async(request).await?;
    Ok(WebSocket { inner: stream })
}

pub trait ReqwestBuilderExt {}

pub fn compat<F>(f: F) -> async_compat::Compat<F> {
    async_compat::Compat::new(f)
}

pub fn project_directories() -> Option<directories::ProjectDirs> {
    directories::ProjectDirs::from("org", "decentraland", "BevyExplorer")
}

pub fn write_config_file(config: &AppConfig) {
    let config_file = project_directories()
        .unwrap()
        .config_dir()
        .join("config.json");

    if let Some(folder) = config_file.parent() {
        std::fs::create_dir_all(folder).unwrap();
    }
    let _ = std::fs::write(config_file, serde_json::to_string(config).unwrap());
}

// re-export prepass types
pub use bevy::core_pipeline::prepass::{DepthPrepass, NormalPrepass};

#[derive(Default)]
pub struct AsyncRwLock<T>(tokio::sync::RwLock<T>);

impl<T> AsyncRwLock<T> {
    pub fn new(value: T) -> Self {
        Self(tokio::sync::RwLock::new(value))
    }

    pub async fn read(&self) -> tokio::sync::RwLockReadGuard<'_, T> {
        self.0.read().await
    }

    pub async fn write(&self) -> tokio::sync::RwLockWriteGuard<'_, T> {
        self.0.write().await
    }

    pub fn blocking_read(&self) -> tokio::sync::RwLockReadGuard<'_, T> {
        self.0.blocking_read()
    }

    pub fn blocking_write(&self) -> tokio::sync::RwLockWriteGuard<'_, T> {
        self.0.blocking_write()
    }

    pub fn try_read(
        &self,
    ) -> Result<tokio::sync::RwLockReadGuard<'_, T>, tokio::sync::TryLockError> {
        self.0.try_read()
    }

    pub fn try_write(
        &self,
    ) -> Result<tokio::sync::RwLockWriteGuard<'_, T>, tokio::sync::TryLockError> {
        self.0.try_write()
    }
}
