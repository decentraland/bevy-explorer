//! Transport boundary for Pulse — the only seam that differs per platform.
//!
//! The driver knows nothing about protobuf, identity, the decoder, or the parcel grid. Its whole
//! contract is: connect to `host:port`, then move bytes between two channels — outbound
//! [`PulseFrame`]s (with a reliability tag the driver maps to ENet channel flags / WebTransport
//! stream-vs-datagram) and inbound raw `ServerMessage` bytes — and report [`PulseStatus`]. Because
//! the seam is bytes, everything above it (handshake, decode, state, glue) is shared and written
//! once, regardless of whether the driver is the native ENet thread or the wasm WebTransport task.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::mpsc;

/// Channel/reliability selector, mirroring the server's `PacketMode`. The driver maps this to its
/// transport's primitive (ENet reliable/unreliable-sequenced/unsequenced; WebTransport
/// stream/datagram).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PulseReliability {
    /// Reliable ordered — handshake, resync, teleport, emotes (server channel 0).
    Reliable,
    /// Unreliable sequenced — high-frequency state input (server channel 1).
    UnreliableSequenced,
    /// Unreliable unordered.
    UnreliableUnsequenced,
}

/// One outbound unit of work: an already-encoded `ClientMessage` plus how to deliver it.
#[derive(Debug)]
pub struct PulseFrame {
    pub bytes: Vec<u8>,
    pub reliability: PulseReliability,
}

/// Connection lifecycle, surfaced from the driver to the protocol layer.
#[derive(Debug, Clone)]
pub enum PulseStatus {
    Connecting,
    Connected,
    Disconnected(String),
    Failed(String),
}

/// Where to connect. Identity / `server_id` / realm live in the protocol layer (`PulseConfig`),
/// not here — the driver only needs an address (and, later for WebTransport, TLS details).
#[derive(Debug, Clone)]
pub struct PulseTransportConfig {
    pub host: String,
    pub port: u16,
}

/// Protocol-layer end of the boundary (held by the Bevy plugin). `Send + Sync`, so it lives in a
/// resource.
pub struct PulseLink {
    pub outbound: mpsc::Sender<PulseFrame>,
    pub inbound: mpsc::Receiver<Vec<u8>>,
    pub status: mpsc::Receiver<PulseStatus>,
}

/// Driver end of the boundary (moved into the native thread / wasm task).
pub struct PulseDriverChannels {
    pub outbound: mpsc::Receiver<PulseFrame>,
    pub inbound: mpsc::Sender<Vec<u8>>,
    pub status: mpsc::Sender<PulseStatus>,
}

/// Build a matched [`PulseLink`] / [`PulseDriverChannels`] pair.
pub fn pulse_channels(capacity: usize) -> (PulseLink, PulseDriverChannels) {
    let (outbound_tx, outbound_rx) = mpsc::channel(capacity);
    let (inbound_tx, inbound_rx) = mpsc::channel(capacity);
    let (status_tx, status_rx) = mpsc::channel(16);

    (
        PulseLink {
            outbound: outbound_tx,
            inbound: inbound_rx,
            status: status_rx,
        },
        PulseDriverChannels {
            outbound: outbound_rx,
            inbound: inbound_tx,
            status: status_tx,
        },
    )
}

/// Owns the running driver. Dropping (or `stop()`) tears it down; on native it also joins the
/// thread.
pub struct PulseDriverHandle {
    stop: Arc<AtomicBool>,
    #[cfg(not(target_arch = "wasm32"))]
    join: Option<std::thread::JoinHandle<()>>,
}

impl PulseDriverHandle {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Drop for PulseDriverHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// Spawn the platform driver. Native runs a dedicated thread (ENet is a synchronous single-thread
/// poll loop); wasm is a no-op today and becomes the WebTransport task later — same boundary.
pub fn spawn_pulse_driver(
    config: PulseTransportConfig,
    channels: PulseDriverChannels,
) -> PulseDriverHandle {
    let stop = Arc::new(AtomicBool::new(false));

    #[cfg(not(target_arch = "wasm32"))]
    {
        let join = super::native::spawn(config, channels, stop.clone());
        PulseDriverHandle {
            stop,
            join: Some(join),
        }
    }

    #[cfg(target_arch = "wasm32")]
    {
        super::wasm::spawn(config, channels);
        PulseDriverHandle { stop }
    }
}
