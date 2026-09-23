use crate::rpc::*;
use pin_project::{pin_project, pinned_drop};
use platform::AsyncRwLock;
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};
use std::{
    future::Future,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};
use tokio_util::sync::CancellationToken;

// the oneshot sender, shared between the local handle and every ipc endpoint registered for
// it; the first send takes it. the receiver sees the channel close once the local handle
// (all its clones) and every endpoint holding the slot have been dropped without a send
type SharedOneshot<T> = Arc<Mutex<Option<tokio::sync::oneshot::Sender<T>>>>;

#[derive(Clone)]
pub enum RpcResultSender<T> {
    Local {
        channel: SharedOneshot<T>,
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

    /// Non-blocking poll. Returns `Some(val)` if a value arrived, `None` if still pending,
    /// or `Err(())` if the sender was dropped without sending.
    #[allow(clippy::result_unit_err)]
    pub fn poll_once(&mut self) -> Result<Option<T>, ()> {
        match self.channel.try_recv() {
            Ok(val) => Ok(Some(val)),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => Ok(None),
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => Err(()),
        }
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
            channel: Arc::new(Mutex::new(None)),
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
                channel: Arc::new(Mutex::new(Some(sx))),
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
                let response = channel.lock().unwrap().take();
                if let Some(response) = response {
                    let _ = response.send(result);
                }
            }
            RpcResultSender::Remote { id, router, .. } => {
                let mut guard = router.blocking_write();
                if let Some(response) = guard.take() {
                    let data = rmp_encode(&result).unwrap();
                    let _ = response.send((*id, IpcMessage::Data(data)));
                }
            }
        }
    }
}

struct IpcResultCallback<T: DeserializeOwned + Send + 'static> {
    sender: SharedOneshot<T>,
}

impl<T: DeserializeOwned + Send + 'static> IpcEndpoint for IpcResultCallback<T> {
    fn send(&mut self, raw_bytes: Vec<u8>) {
        if let Ok(val) = rmp_serde::from_slice::<T>(&raw_bytes) {
            let sx = self.sender.lock().unwrap().take();
            if let Some(sx) = sx {
                let _ = sx.send(val);
            }
        } else {
            let _ = rmp_serde::from_slice::<T>(&raw_bytes).unwrap();
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

        // every serialization gets its own endpoint and id, so each remote sender's lifetime
        // is tracked independently and dropping one never closes an endpoint another uses
        let endpoint = IpcResultCallback {
            sender: channel.clone(),
        };
        let (id, close_sender, removed) = ipc_register(endpoint);
        spawn_close_watcher(id, cancel.clone(), removed, close_sender);
        debug!("created sender {id} -> {}", std::any::type_name::<T>());

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
            debug!("last dropped {id} - {}", std::any::type_name::<T>());
            ipc_router_close(id, &cancel_router);
        });

        Ok(Self::Remote {
            id,
            router: Arc::new(AsyncRwLock::new(Some(router))),
            receiver_dropped: cancel,
            sender_alive: sx,
        })
    }
}
