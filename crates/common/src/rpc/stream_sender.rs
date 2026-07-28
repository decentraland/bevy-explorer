use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

use crate::rpc::*;
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};
use tokio_util::sync::CancellationToken;

// In-flight counter for a bounded stream: the sender drops when full, the receiver decrements as it drains.
#[derive(Debug)]
struct QueueGaugeInner {
    len: AtomicUsize,
    cap: usize,
}

type QueueGauge = Arc<QueueGaugeInner>;

#[derive(Clone)]
pub enum LocalChannel<T> {
    Channel(tokio::sync::mpsc::UnboundedSender<T>),
    Serialized(u64),
}

impl<T> LocalChannel<T> {
    fn serialize_with<F: FnOnce(tokio::sync::mpsc::UnboundedSender<T>) -> u64>(
        &mut self,
        f: F,
    ) -> u64 {
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
        gauge: Option<QueueGauge>,
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
    gauge: Option<QueueGauge>,
}

impl<T> RpcStreamReceiver<T> {
    pub fn try_recv(&mut self) -> Result<T, tokio::sync::mpsc::error::TryRecvError> {
        let result = self.channel.try_recv();
        if result.is_ok() {
            if let Some(gauge) = &self.gauge {
                gauge.len.fetch_sub(1, Ordering::Relaxed);
            }
        }
        result
    }

    pub async fn recv(&mut self) -> Option<T> {
        let result = self.channel.recv().await;
        if result.is_some() {
            if let Some(gauge) = &self.gauge {
                gauge.len.fetch_sub(1, Ordering::Relaxed);
            }
        }
        result
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
                gauge: None,
            },
            RpcStreamReceiver {
                channel: rx,
                cancel,
                gauge: None,
            },
        )
    }

    // Caps in-flight items at `cap`; once full the sender drops further messages (fail closed).
    pub fn bounded_channel(cap: usize) -> (Self, RpcStreamReceiver<T>) {
        let (sx, rx) = tokio::sync::mpsc::unbounded_channel();
        let cancel = CancellationToken::new();
        let gauge: QueueGauge = Arc::new(QueueGaugeInner {
            len: AtomicUsize::new(0),
            cap: cap.max(1),
        });

        (
            Self::Local {
                channel: Arc::new(Mutex::new(LocalChannel::Channel(sx))),
                cancel: cancel.clone(),
                gauge: Some(gauge.clone()),
            },
            RpcStreamReceiver {
                channel: rx,
                cancel,
                gauge: Some(gauge),
            },
        )
    }

    pub fn send(&self, val: T) -> Result<(), tokio::sync::mpsc::error::SendError<T>> {
        match self {
            RpcStreamSender::Local { channel, gauge, .. } => {
                if let Some(gauge) = gauge {
                    if gauge.len.load(Ordering::Relaxed) >= gauge.cap {
                        return Ok(());
                    }
                }
                match &*channel.lock().unwrap() {
                    LocalChannel::Channel(unbounded_sender) => {
                        let result = unbounded_sender.send(val);
                        if result.is_ok() {
                            if let Some(gauge) = gauge {
                                gauge.len.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        result
                    }
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
            RpcStreamSender::Local { channel, .. } => match &*channel.lock().unwrap() {
                LocalChannel::Channel(unbounded_sender) => unbounded_sender.is_closed(),
                LocalChannel::Serialized(_) => panic!(),
            },
            RpcStreamSender::Remote {
                receiver_dropped: close_token,
                ..
            } => close_token.is_cancelled(),
        }
    }
}

struct IpcStreamCallback<T: DeserializeOwned + Send + 'static> {
    sender: tokio::sync::mpsc::UnboundedSender<T>,
    gauge: Option<QueueGauge>,
}

impl<T: DeserializeOwned + Send + 'static> IpcEndpoint for IpcStreamCallback<T> {
    fn send(&mut self, raw_bytes: Vec<u8>) {
        if let Ok(val) = rmp_serde::from_slice::<T>(&raw_bytes) {
            if let Some(gauge) = &self.gauge {
                if gauge.len.load(Ordering::Relaxed) >= gauge.cap {
                    return;
                }
            }
            if self.sender.send(val).is_ok() {
                if let Some(gauge) = &self.gauge {
                    gauge.len.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }
}

impl<T: 'static + Serialize + DeserializeOwned + Send> Serialize for RpcStreamSender<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let RpcStreamSender::Local {
            channel,
            cancel,
            gauge,
        } = self
        else {
            panic!();
        };

        let gauge = gauge.clone();
        let id = channel.lock().unwrap().serialize_with(move |sender| {
            let endpoint = IpcStreamCallback { sender, gauge };
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
