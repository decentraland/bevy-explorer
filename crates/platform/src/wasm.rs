use std::time::Duration;

pub use tungstenite::client::IntoClientRequest;

pub struct WebSocket {}

impl WebSocket {
    pub async fn send(&mut self, item: tungstenite::Message) -> Result<(), anyhow::Error> {
        todo!()
    }

    pub async fn next(&mut self) -> Option<Result<tungstenite::Message, anyhow::Error>> {
        todo!()
    }

    pub fn split(self) -> (WebSocketSplitSink, WebSocketSplitStream) {
        todo!()
    }
}

pub struct WebSocketSplitSink {}

impl WebSocketSplitSink {
    pub async fn send(&mut self, item: tungstenite::Message) -> Result<(), anyhow::Error> {
        todo!()
    }
}

pub struct WebSocketSplitStream {}

impl WebSocketSplitStream {
    pub async fn next(&mut self) -> Option<Result<tungstenite::Message, anyhow::Error>> {
        todo!()
    }
}

pub async fn websocket<R>(request: R) -> Result<WebSocket, anyhow::Error>
where
    R: IntoClientRequest + Unpin,
{
    todo!()
}

pub trait ReqwestBuilderExt {
    fn timeout(self, timeout: Duration) -> Self;
    fn connect_timeout(self, timeout: Duration) -> Self;
    fn use_native_tls(self) -> Self;
}

impl ReqwestBuilderExt for reqwest::ClientBuilder {
    fn timeout(self, timeout: Duration) -> Self {
        self
    }

    fn connect_timeout(self, timeout: Duration) -> Self {
        self
    }
    
    fn use_native_tls(self) -> Self {
        self
    }
}

impl ReqwestBuilderExt for reqwest::RequestBuilder {
    fn timeout(self, timeout: Duration) -> Self {
        self
    }

    fn connect_timeout(self, timeout: Duration) -> Self {
        self
    }
    
    fn use_native_tls(self) -> Self {
        self
    }
}
