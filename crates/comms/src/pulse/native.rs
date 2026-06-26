//! Native Pulse driver — a dedicated OS thread running the ENet service loop.
//!
//! ENet is a synchronous, single-threaded poll loop (`enet_host_service` blocks briefly and
//! returns one event; the host is not thread-safe), so it gets its own thread that owns the host
//! and bridges to the Bevy side over [`PulseDriverChannels`]. This mirrors the server's own
//! dedicated-thread rule. No async runtime is involved.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::transport::{PulseDisconnect, PulseDriverChannels, PulseStatus, PulseTransportConfig};

pub(super) fn spawn(
    config: PulseTransportConfig,
    channels: PulseDriverChannels,
    stop: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name("pulse-enet".into())
        .spawn(move || run(config, channels, &stop))
        .expect("failed to spawn pulse-enet thread")
}

fn run(config: PulseTransportConfig, mut channels: PulseDriverChannels, stop: &AtomicBool) {
    let _ = channels.status.try_send(PulseStatus::Connecting);

    // TODO(pulse-enet): link the native ENet lib (matching the server's bundled fork), initialize
    // the library, create a host, and connect to `config.host:config.port` on 3 channels. Until
    // that lands the driver is inert — it drains outbound so the protocol layer never backs up,
    // and never produces inbound traffic (foreign players keep arriving over LiveKit).
    bevy::log::warn!(
        "pulse: native ENet driver not yet wired (target {}:{}); running inert",
        config.host,
        config.port
    );

    while !stop.load(Ordering::Relaxed) {
        // Discard outbound until ENet send is wired, so PulseFrame producers don't back up.
        while channels.outbound.try_recv().is_ok() {}

        // TODO(pulse-enet): replace the sleep with `host.service(1ms)`. On Receive →
        // channels.inbound.try_send(bytes); on Connect → status Connected; on Disconnect/Timeout →
        // status Disconnected(PulseDisconnect::from_code(event.data)) + return.
        thread::sleep(Duration::from_millis(16));
    }

    // Stop requested by the protocol layer (session dropped) — a clean local shutdown.
    let _ = channels
        .status
        .try_send(PulseStatus::Disconnected(PulseDisconnect::Graceful));
}
