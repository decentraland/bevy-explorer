//! Native Pulse driver — a dedicated OS thread running a current-thread Tokio runtime around the
//! ENet host.
//!
//! ENet is a synchronous, non-thread-safe poll loop, so it gets its own thread that owns the host.
//! Rather than busy-polling, the thread blocks in [`tokio::select!`] on the two work sources — the
//! outbound [`PulseFrame`] channel and UDP-socket readability — plus a coarse maintenance tick so
//! ENet's own timers (pings, reliable retransmits) keep ticking when otherwise idle. After any
//! wake it drains outbound onto the peer and services the host. The host's socket is a Tokio
//! [`UdpSocket`] driven non-blocking via `try_recv_from` / `try_send_to`.
//!
//! The host is a (locally patched) [`rusty_enet`] — a pure-Rust port of ENet retargeted to the
//! nxrighthere/SoftwareGuy ENet-CSharp "modified protocol" fork the server runs (the patch is the
//! three header-flag constants in the fork's `src/c/protocol.rs`). No compressor and no checksum,
//! matching the server's host exactly.

use std::io::{self, ErrorKind};
use std::net::{Ipv4Addr, SocketAddr, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rusty_enet as enet;
use tokio::net::UdpSocket;

use super::transport::{
    PulseDisconnect, PulseDriverChannels, PulseFrame, PulseReliability, PulseStatus,
    PulseTransportConfig,
};

/// Channels in server order: RELIABLE=0, UNRELIABLE_SEQUENCED=1, UNRELIABLE_UNSEQUENCED=2
/// (the server's `ENetChannel.COUNT`).
const CHANNEL_COUNT: usize = 3;

/// How long to block with no inbound/outbound activity before servicing the host anyway, so ENet's
/// pings and reliable retransmits keep ticking on an otherwise-idle connection.
const MAINTENANCE_INTERVAL: Duration = Duration::from_millis(100);

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

/// Tokio [`UdpSocket`] adapter implementing rusty_enet's [`enet::Socket`]. The newtype is required
/// by the orphan rule; the methods just bridge to Tokio's non-blocking datagram ops.
struct PulseSocket(UdpSocket);

impl enet::Socket for PulseSocket {
    type Address = SocketAddr;
    type Error = io::Error;

    fn init(&mut self, _options: enet::SocketOptions) -> io::Result<()> {
        // Tokio sockets are already non-blocking; match the std impl's broadcast flag.
        self.0.set_broadcast(true)
    }

    fn send(&mut self, address: SocketAddr, buffer: &[u8]) -> io::Result<usize> {
        match self.0.try_send_to(buffer, address) {
            Ok(sent) => Ok(sent),
            Err(err) if err.kind() == ErrorKind::WouldBlock => Ok(0),
            Err(err) => Err(err),
        }
    }

    fn receive(
        &mut self,
        buffer: &mut [u8; enet::MTU_MAX],
    ) -> io::Result<Option<(SocketAddr, enet::PacketReceived)>> {
        match self.0.try_recv_from(buffer) {
            Ok((len, address)) => Ok(Some((address, enet::PacketReceived::Complete(len)))),
            Err(err) if err.kind() == ErrorKind::WouldBlock => Ok(None),
            Err(err) => Err(err),
        }
    }
}

fn run(config: PulseTransportConfig, mut channels: PulseDriverChannels, stop: &AtomicBool) {
    let _ = channels.status.try_send(PulseStatus::Connecting);

    // Resolve the server address (DNS). A failure here is never-established → Failed (retryable).
    let address = match (config.host.as_str(), config.port).to_socket_addrs() {
        Ok(mut addrs) => match addrs.next() {
            Some(address) => address,
            None => return fail(&mut channels, format!("no address for {}", config.host)),
        },
        Err(err) => {
            return fail(
                &mut channels,
                format!("resolve {} failed: {err}", config.host),
            )
        }
    };

    // One OS thread, one current-thread runtime — keeps ENet single-threaded while letting us block
    // on socket/channel readiness instead of spinning.
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => return fail(&mut channels, format!("tokio runtime build failed: {err}")),
    };
    runtime.block_on(drive(&mut channels, address, stop));
}

async fn drive(channels: &mut PulseDriverChannels, address: SocketAddr, stop: &AtomicBool) {
    // Ephemeral local UDP socket; ENet drives it non-blocking via `PulseSocket`.
    let socket = match UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await {
        Ok(socket) => PulseSocket(socket),
        Err(err) => return fail(channels, format!("udp bind failed: {err}")),
    };

    // One peer (the server), three channels, no compressor/checksum (`HostSettings` defaults).
    let mut host = match enet::Host::new(
        socket,
        enet::HostSettings {
            peer_limit: 1,
            channel_limit: CHANNEL_COUNT,
            ..Default::default()
        },
    ) {
        Ok(host) => host,
        Err(err) => return fail(channels, format!("enet host create failed: {err}")),
    };

    let peer = match host.connect(address, CHANNEL_COUNT, 0) {
        Ok(peer) => peer.id(),
        Err(err) => return fail(channels, format!("enet connect failed: {err}")),
    };

    while !stop.load(Ordering::Relaxed) {
        // Block until there's work: an outbound frame to send, an inbound datagram to read, or the
        // maintenance deadline so ENet's timers keep ticking.
        let outbound = tokio::select! {
            frame = channels.outbound.recv() => Some(frame),
            _ = host.socket().0.readable() => None,
            _ = tokio::time::sleep(MAINTENANCE_INTERVAL) => None,
        };
        match outbound {
            // Channel closed — the protocol layer dropped the link; stop the driver.
            Some(None) => break,
            Some(Some(frame)) => queue_frame(&mut host, peer, &frame),
            None => {}
        }

        // Drain any further queued outbound without blocking; the next service flushes them.
        while let Ok(frame) = channels.outbound.try_recv() {
            queue_frame(&mut host, peer, &frame);
        }

        // Service the host: socket I/O + dispatch all ready events.
        loop {
            match host.service() {
                Ok(Some(enet::Event::Connect { .. })) => {
                    let _ = channels.status.try_send(PulseStatus::Connected);
                }
                Ok(Some(enet::Event::Disconnect { data, .. })) => {
                    let reason = PulseDisconnect::from_code(data);
                    let _ = channels.status.try_send(PulseStatus::Disconnected(reason));
                    return;
                }
                Ok(Some(enet::Event::Receive { packet, .. })) => {
                    let _ = channels.inbound.try_send(packet.data().to_vec());
                }
                Ok(None) => break,
                Err(err) => return fail(channels, format!("enet service error: {err}")),
            }
        }
    }

    // Stop requested by the protocol layer (session dropped) — disconnect cleanly and flush.
    host.peer_mut(peer).disconnect(0);
    host.flush();
    let _ = channels
        .status
        .try_send(PulseStatus::Disconnected(PulseDisconnect::Graceful));
}

/// Queue one outbound [`PulseFrame`] onto the peer. Channel and packet kind together reproduce the
/// server's `ENetChannel` wire commands: reliable → `SEND_RELIABLE`, unreliable-sequenced →
/// `SEND_UNRELIABLE`, unreliable-unsequenced → `SEND_UNSEQUENCED`.
fn queue_frame(host: &mut enet::Host<PulseSocket>, peer: enet::PeerID, frame: &PulseFrame) {
    let (channel_id, packet) = match frame.reliability {
        PulseReliability::Reliable => (0, enet::Packet::reliable(frame.bytes.as_slice())),
        PulseReliability::UnreliableSequenced => {
            (1, enet::Packet::unreliable(frame.bytes.as_slice()))
        }
        PulseReliability::UnreliableUnsequenced => (
            2,
            enet::Packet::unreliable_unsequenced(frame.bytes.as_slice()),
        ),
    };
    if let Err(err) = host.peer_mut(peer).send(channel_id, &packet) {
        bevy::log::warn!("pulse: peer send failed: {err}");
    }
}

/// Report a never-established failure (DNS/socket/connect). Always transient — the protocol layer
/// retries.
fn fail(channels: &mut PulseDriverChannels, message: String) {
    bevy::log::warn!("pulse: {message}");
    let _ = channels.status.try_send(PulseStatus::Failed(message));
}
