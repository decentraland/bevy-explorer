use crate::rpc::*;
use pin_project::{pin_project, pinned_drop};
use platform::AsyncRwLock;
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};
use std::{
    future::Future,
    sync::Arc,
    task::{Context, Poll},
};
use tokio_util::sync::CancellationToken;

pub enum LocalChannel<T> {
    Channel(tokio::sync::oneshot::Sender<T>),
    Serialized(u64),
    Used,
}

impl<T> LocalChannel<T> {
    fn serialize_with<F: FnOnce(tokio::sync::oneshot::Sender::<T>) -> u64>(&mut self, f: F) -> u64 {        
        let id = match std::mem::replace(self, LocalChannel::Used) {
            LocalChannel::Channel(sender) => (f)(sender),
            LocalChannel::Serialized(id) => id,
            LocalChannel::Used => panic!(),
        };

        *self = LocalChannel::Serialized(id);
        id
    }

    fn take(&mut self) -> Option<tokio::sync::oneshot::Sender<T>> {
        match std::mem::replace(self, LocalChannel::Used) {
            LocalChannel::Channel(sender) => Some(sender),
            LocalChannel::Serialized(_) => panic!(),
            LocalChannel::Used => None,
        }
    }
}

#[derive(Clone)]
pub enum RpcResultSender<T> {
    Local {
        channel: Arc<AsyncRwLock<LocalChannel<T>>>,
        cancel: CancellationToken,
    },
    Remote {
        id: u64,
        #[allow(clippy::type_complexity)]
        router: Arc<AsyncRwLock<Option<tokio::sync::mpsc::UnboundedSender<(u64, IpcMessage)>>>>,
        receiver_dropped: CancellationToken,
        sender_alive: tokio::sync::mpsc::Sender<()>,
    },
}

#[pin_project(PinnedDrop)]
pub struct RpcResultReceiver<T> {
    #[pin]
    channel: tokio::sync::oneshot::Receiver<T>,
    cancel: CancellationToken,
}

impl<T> RpcResultReceiver<T> {
    pub fn try_recv(&mut self) -> Result<T, tokio::sync::oneshot::error::TryRecvError> {
        self.channel.try_recv()
    }
}

impl<T> Future for RpcResultReceiver<T> {
    type Output = Result<T, tokio::sync::oneshot::error::RecvError>;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        this.channel.poll(cx)
    }
}

#[pinned_drop]
impl<T> PinnedDrop for RpcResultReceiver<T> {
    fn drop(mut self: std::pin::Pin<&mut Self>) {
        if !self.channel.is_terminated() {
            self.cancel.cancel();
        }
    }
}

impl<T> std::fmt::Debug for RpcResultSender<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("RpcResultSender").finish()
    }
}

impl<T: 'static> Default for RpcResultSender<T> {
    fn default() -> Self {
        Self::Local {
            channel: Arc::new(AsyncRwLock::new(LocalChannel::Used)),
            cancel: CancellationToken::new(),
        }
    }
}

impl<T: Serialize + 'static> RpcResultSender<T> {
    pub fn channel() -> (Self, RpcResultReceiver<T>) {
        let (sx, rx) = tokio::sync::oneshot::channel();
        let cancel = CancellationToken::new();

        (
            Self::Local {
                channel: Arc::new(AsyncRwLock::new(LocalChannel::Channel(sx))),
                cancel: cancel.clone(),
            },
            RpcResultReceiver {
                channel: rx,
                cancel,
            },
        )
    }

    pub fn send(&self, result: T) {
        match self {
            RpcResultSender::Local { channel, .. } => {
                let mut guard = channel.blocking_write();
                if let Some(response) = guard.take() {
                    let _ = response.send(result);
                }
            }
            RpcResultSender::Remote { id, router, .. } => {
                let mut guard = router.blocking_write();
                if let Some(response) = guard.take() {
                    let data = bincode::serialize(&result).unwrap();
                    let _ = response.send((*id, IpcMessage::Data(data)));
                }
            }
        }
    }
}

struct IpcResultCallback<T: DeserializeOwned + Send + 'static> {
    sender: Option<tokio::sync::oneshot::Sender<T>>,
}

impl<T: DeserializeOwned + Send + 'static> IpcEndpoint for IpcResultCallback<T> {
    fn send(&mut self, raw_bytes: Vec<u8>) {
        if let Ok(val) = bincode::deserialize::<T>(&raw_bytes) {
            if let Some(sx) = self.sender.take() {
                let _ = sx.send(val);
            }
        } else {
            let _ = bincode::deserialize::<T>(&raw_bytes).unwrap();
        }
    }
}

impl<T: 'static + Serialize + DeserializeOwned + Send> Serialize for RpcResultSender<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let RpcResultSender::Local { channel, cancel } = self else {
            panic!();
        };

        let id = channel.try_write().unwrap().serialize_with(|sender| {
            let endpoint = IpcResultCallback {
                sender: Some(sender),
            };
            let (id, close_sender) = ipc_register(endpoint);
            let cancel = cancel.clone();
            tokio::spawn(async move {
                cancel.cancelled().await;
                let _ = close_sender.send(id);
            });
            info!("created sender {id} -> {}", std::any::type_name::<T>());
            id
        });

        serializer.serialize_u64(id)
    }
}

impl<'de, T> Deserialize<'de> for RpcResultSender<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let id = u64::deserialize(deserializer)?;
        let (router, cancel) = ipc_router(id);
        let (sx, mut rx) = tokio::sync::mpsc::channel(1);

        let cancel_router = router.clone();
        tokio::spawn(async move {
            rx.recv().await; // block till all senders are dropped
            info!("last dropped {id} - {}", std::any::type_name::<T>());
            let _ = cancel_router.send((id, IpcMessage::Closed));
        });

        Ok(Self::Remote {
            id,
            router: Arc::new(AsyncRwLock::new(Some(router))),
            receiver_dropped: cancel,
            sender_alive: sx,
        })
    }
}
