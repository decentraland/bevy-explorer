use std::sync::{Arc, Mutex};

use crate::rpc::*;
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub enum LocalChannel<T> {
    Channel(tokio::sync::mpsc::UnboundedSender<T>),
    Serialized(u64),
}

impl<T> LocalChannel<T> {
    fn serialize_with<F: FnOnce(tokio::sync::mpsc::UnboundedSender::<T>) -> u64>(&mut self, f: F) -> u64 {        
        let id = match std::mem::replace(self, LocalChannel::Serialized(u64::MAX)) {
            LocalChannel::Channel(sender) => (f)(sender),
            LocalChannel::Serialized(id) => id,
        };

        *self = LocalChannel::Serialized(id);
        id
    }
}


#[derive(Clone)]
pub enum RpcStreamSender<T> {
    Local {
        channel: Arc<Mutex<LocalChannel<T>>>,
        cancel: CancellationToken,
    },
    Remote {
        id: u64,
        router: tokio::sync::mpsc::UnboundedSender<(u64, IpcMessage)>,
        receiver_dropped: CancellationToken,
        sender_alive: tokio::sync::mpsc::Sender<()>,
    },
}

impl<T> std::fmt::Debug for RpcStreamSender<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("RpcStreamSender").finish()
    }
}

pub struct RpcStreamReceiver<T> {
    channel: tokio::sync::mpsc::UnboundedReceiver<T>,
    cancel: CancellationToken,
}

impl<T> RpcStreamReceiver<T> {
    pub fn try_recv(&mut self) -> Result<T, tokio::sync::mpsc::error::TryRecvError> {
        self.channel.try_recv()
    }

    pub async fn recv(&mut self) -> Option<T> {
        self.channel.recv().await
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
                channel: Arc::new(Mutex::new(LocalChannel::Channel(sx))),
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
            RpcStreamSender::Local { channel, .. } => {
                match &*channel.lock().unwrap() {
                    LocalChannel::Channel(unbounded_sender) => unbounded_sender.send(val),
                    LocalChannel::Serialized(_) => panic!(),
                }
            }
            RpcStreamSender::Remote {
                id,
                router,
                receiver_dropped,
                ..
            } => {
                if receiver_dropped.is_cancelled() {
                    return Err(tokio::sync::mpsc::error::SendError(val));
                }
                let data = rmp_encode(&val).unwrap();
                router
                    .send((*id, IpcMessage::Data(data)))
                    .map_err(|_| tokio::sync::mpsc::error::SendError(val))
            }
        }
    }

    pub fn is_closed(&self) -> bool {
        match self {
            RpcStreamSender::Local { channel, .. } => {
                match &*channel.lock().unwrap() {
                    LocalChannel::Channel(unbounded_sender) => unbounded_sender.is_closed(),
                    LocalChannel::Serialized(_) => panic!(),
                }
            }
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
        if let Ok(val) = rmp_serde::from_slice::<T>(&raw_bytes) {
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

        let id = channel.lock().unwrap().serialize_with(|sender| {
            let endpoint = IpcStreamCallback {
                sender,
            };
            let (id, close_sender) = ipc_register(endpoint);

            let cancel = cancel.clone();
            tokio::spawn(async move {
                cancel.cancelled().await;
                let _ = close_sender.send(id);
            });

            id
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
