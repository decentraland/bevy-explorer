use crate::rpc::*;
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub enum RpcStreamSender<T> {
    Local {
        channel: tokio::sync::mpsc::UnboundedSender<T>,
        cancel: CancellationToken,
    },
    Remote {
        id: u64,
        router: tokio::sync::mpsc::UnboundedSender<(u64, IpcMessage)>,
        receiver_dropped: CancellationToken,
        sender_alive: tokio::sync::mpsc::Sender<()>,
    },
}

pub struct RpcStreamReceiver<T> {
    channel: tokio::sync::mpsc::UnboundedReceiver<T>,
    cancel: CancellationToken,
}

impl<T> RpcStreamReceiver<T> {
    pub fn try_recv(&mut self) -> Result<T, tokio::sync::mpsc::error::TryRecvError> {
        self.channel.try_recv()
    }
}

impl<T> Drop for RpcStreamReceiver<T> {
    fn drop(&mut self) {
        if !self.channel.is_closed() {
            self.cancel.cancel();
        }
    }
}

impl<T: Serialize> RpcStreamSender<T> {
    pub fn channel() -> (Self, RpcStreamReceiver<T>) {
        let (sx, rx) = tokio::sync::mpsc::unbounded_channel();
        let cancel = CancellationToken::new();

        (
            Self::Local {
                channel: sx,
                cancel: cancel.clone(),
            },
            RpcStreamReceiver {
                channel: rx,
                cancel,
            },
        )
    }

    pub fn send(&self, val: T) -> Result<(), tokio::sync::mpsc::error::SendError<T>> {
        match self {
            RpcStreamSender::Local { channel, .. } => channel.send(val),
            RpcStreamSender::Remote {
                id,
                router,
                receiver_dropped,
                ..
            } => {
                if receiver_dropped.is_cancelled() {
                    return Err(tokio::sync::mpsc::error::SendError(val));
                }
                let data = bincode::serialize(&val).unwrap();
                router
                    .send((*id, IpcMessage::Data(data)))
                    .map_err(|_| tokio::sync::mpsc::error::SendError(val))
            }
        }
    }

    pub fn is_closed(&self) -> bool {
        match self {
            RpcStreamSender::Local { channel, .. } => channel.is_closed(),
            RpcStreamSender::Remote {
                receiver_dropped: close_token,
                ..
            } => close_token.is_cancelled(),
        }
    }
}

struct IpcStreamCallback<T: DeserializeOwned + Send + 'static> {
    sender: tokio::sync::mpsc::UnboundedSender<T>,
}

impl<T: DeserializeOwned + Send + 'static> IpcEndpoint for IpcStreamCallback<T> {
    fn send(&mut self, raw_bytes: Vec<u8>) {
        if let Ok(val) = bincode::deserialize::<T>(&raw_bytes) {
            let _ = self.sender.send(val);
        }
    }
}

impl<T: 'static + Serialize + DeserializeOwned + Send> Serialize for RpcStreamSender<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let RpcStreamSender::Local { channel, cancel } = self else {
            panic!();
        };

        let endpoint = IpcStreamCallback {
            sender: channel.clone(),
        };
        let (id, close_sender) = ipc_register(endpoint);

        let cancel = cancel.clone();
        tokio::spawn(async move {
            cancel.cancelled().await;
            let _ = close_sender.send(id);
        });

        serializer.serialize_u64(id)
    }
}

impl<'de, T> Deserialize<'de> for RpcStreamSender<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let id = u64::deserialize(deserializer)?;
        let (router, close_channel) = ipc_router(id);
        let (sx, mut rx) = tokio::sync::mpsc::channel(1);

        let cancel_router = router.clone();
        tokio::spawn(async move {
            rx.recv().await; // block till all senders are dropped
            let _ = cancel_router.send((id, IpcMessage::Closed));
        });

        Ok(Self::Remote {
            id,
            router,
            receiver_dropped: close_channel,
            sender_alive: sx,
        })
    }
}
