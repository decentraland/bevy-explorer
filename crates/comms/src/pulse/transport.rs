//! Transport boundary for Pulse — the only seam that differs per platform.
//!
//! The driver knows nothing about protobuf, identity, the decoder, or the parcel grid. Its whole
//! contract is: connect to `host:port`, then move bytes between two channels — outbound
//! [`PulseFrame`]s (with a reliability tag the driver maps to ENet channel flags / WebTransport
//! stream-vs-datagram) and inbound raw `ServerMessage` bytes — and report [`PulseStatus`]. Because
//! the seam is bytes, everything above it (handshake, decode, state, glue) is shared and written
//! once, regardless of whether the driver is the native ENet thread or the wasm WebTransport task.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};

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
    /// Connected-then-dropped, carrying the server's reason (see [`PulseDisconnect`]).
    Disconnected(PulseDisconnect),
    /// Never established (DNS/socket/connect timeout) — always transient, safe to retry.
    Failed(String),
}

/// Server disconnect reason. This is an out-of-band ABI shared with the server
/// (`Pulse.Transport.DisconnectReason`), *not* a wire message: it arrives as the ENet disconnect
/// event's `data` code. The driver maps that `u32` here so the protocol layer can decide whether a
/// reconnect could plausibly help — several reasons are terminal by the server's design (bad auth,
/// ban, eviction, flagged misbehaviour) and must not trigger a reconnect loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PulseDisconnect {
    /// No reason supplied (e.g. a plain network timeout where the remote set no code).
    None,
    /// Clean shutdown / server stopping.
    Graceful,
    /// Our `PENDING_AUTH` deadline was exceeded (handshake too slow).
    AuthTimeout,
    /// Handshake validation failed.
    AuthFailed,
    /// Evicted by a newer connection with the same wallet — retrying fights that session.
    DuplicateSession,
    /// Banned platform-wide.
    Banned,
    /// Server at capacity.
    ServerFull,
    /// Per-source-IP pre-auth connection cap exceeded.
    PreAuthIpLimit,
    /// Global pre-auth budget exhausted.
    PreAuthBudget,
    /// Sent `PlayerStateInput` faster than the server's cap (misbehaving client).
    InputRateExceeded,
    /// Exceeded the discrete-event (emote/teleport) cap (misbehaving client).
    DiscreteEventRateExceeded,
    /// `PlayerStateInput` carried an invalid field.
    InvalidInputField,
    /// `EmoteStart` carried an invalid field.
    InvalidEmoteField,
    /// `TeleportRequest` carried an invalid field (oversized/empty realm, bad parcel).
    InvalidTeleportField,
    /// A handshake with the same (wallet, timestamp) was replayed inside the anti-replay window.
    HandshakeReplayRejected,
    /// `HandshakeRequest` carried a malformed `PlayerInitialState`.
    InvalidHandshakeField,
    /// Sustained corrupt/oversized packets (buggy client, fuzzer, or amplification probe).
    PacketCorrupted,
    /// A code this client build doesn't recognise. Treated as terminal to be safe.
    Unknown(u32),
}

impl PulseDisconnect {
    /// Map an ENet disconnect `data` code to a reason. Mirrors the server's `DisconnectReason` enum
    /// 1:1; unrecognised codes (a newer server) fall through to [`PulseDisconnect::Unknown`].
    pub fn from_code(code: u32) -> Self {
        match code {
            0 => Self::None,
            1 => Self::Graceful,
            2 => Self::AuthTimeout,
            3 => Self::AuthFailed,
            4 => Self::DuplicateSession,
            5 => Self::Banned,
            6 => Self::ServerFull,
            7 => Self::PreAuthIpLimit,
            8 => Self::PreAuthBudget,
            9 => Self::InputRateExceeded,
            10 => Self::DiscreteEventRateExceeded,
            11 => Self::InvalidInputField,
            12 => Self::InvalidEmoteField,
            13 => Self::InvalidTeleportField,
            14 => Self::HandshakeReplayRejected,
            15 => Self::InvalidHandshakeField,
            16 => Self::PacketCorrupted,
            other => Self::Unknown(other),
        }
    }

    /// Whether reconnecting could plausibly succeed. Only transient, server-side, or
    /// too-slow-this-time reasons are retryable; auth/ban/eviction/misbehaviour reasons (and any
    /// unrecognised code) are terminal — reconnecting against them just loops.
    pub fn should_retry(self) -> bool {
        matches!(
            self,
            Self::None
                | Self::Graceful
                | Self::AuthTimeout
                | Self::ServerFull
                | Self::PreAuthIpLimit
                | Self::PreAuthBudget
        )
    }
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
    /// Liveness handle for the realm's Pulse routing entity. A weak ref to the session's `Arc<()>`
    /// anchor; the routing entity holds a strong clone while it exists, so `strong_count() > 1` means
    /// "currently on a Pulse realm". The driver reads this to decide whether to surface inbound peer
    /// state (it stays connected across realms either way). A `Weak` so the driver never keeps the
    /// session alive, and a fresh one is handed to each rebuilt driver so reconnects don't lose it.
    pub presence: Weak<()>,
}

/// Build a matched [`PulseLink`] / [`PulseDriverChannels`] pair. `presence` is the driver's liveness
/// handle (see [`PulseDriverChannels::presence`]).
pub fn pulse_channels(capacity: usize, presence: Weak<()>) -> (PulseLink, PulseDriverChannels) {
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
            presence,
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
