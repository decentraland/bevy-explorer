use async_tungstenite::{async_std::ConnectStream, WebSocketStream};
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
