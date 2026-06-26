//! Pulse Bevy plugin — the shared, platform-agnostic protocol layer.
//!
//! Owns the [`PulseDecoder`] and the driver lifecycle, and pumps the byte boundary: inbound
//! `ServerMessage` bytes are decoded and dispatched; outbound `ClientMessage`s (resync today;
//! handshake + input later) are encoded onto the driver. The driver itself (native thread / wasm
//! task) is selected at compile time and never seen here.

use bevy::prelude::*;
use dcl_component::proto_components::kernel::comms::rfc4::packet::Message;
use dcl_component::proto_components::pulse;
use prost::Message as _;
use tokio::sync::mpsc;

use super::transport::{
    self, PulseDriverHandle, PulseFrame, PulseLink, PulseReliability, PulseStatus,
    PulseTransportConfig,
};
use super::{PulseDecoder, PulseEvent, PulseParcelGrid};
use crate::global_crdt::{GlobalCrdtState, NetworkUpdate, PlayerMessage, PlayerUpdate};

/// Insert this resource to connect to a Pulse server. Absent → the plugin is fully inert.
#[derive(Resource, Clone)]
pub struct PulseConfig {
    pub transport: PulseTransportConfig,
    pub parcel_grid: PulseParcelGrid,
    /// Identifies this server instance; folded into the handshake connect signature.
    pub server_id: String,
}

#[derive(Resource)]
struct PulseSession {
    link: PulseLink,
    decoder: PulseDecoder,
    /// Sink into the shared foreign-player pipeline — the same channel every transport feeds.
    sender: mpsc::Sender<NetworkUpdate>,
    /// Synthetic transport entity used as the foreign players' `transport_id`.
    transport: Entity,
    /// Held for the session lifetime; dropping it stops the driver.
    _driver: PulseDriverHandle,
}

/// Marker for the synthetic Pulse transport entity (referenced by foreign players' `transport_id`).
#[derive(Component)]
struct PulseTransport;

pub struct PulsePlugin;

impl Plugin for PulsePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (connect_pulse, pump_pulse).chain());
    }
}

/// Bring a session up once a [`PulseConfig`] is present. No-op afterwards (session exists).
fn connect_pulse(
    mut commands: Commands,
    crdt: Res<GlobalCrdtState>,
    config: Option<Res<PulseConfig>>,
    session: Option<Res<PulseSession>>,
) {
    let (Some(config), None) = (config, session) else {
        return;
    };

    let (link, driver_channels) = transport::pulse_channels(1024);
    let driver = transport::spawn_pulse_driver(config.transport.clone(), driver_channels);
    let transport = commands.spawn(PulseTransport).id();

    commands.insert_resource(PulseSession {
        link,
        decoder: PulseDecoder::new(config.parcel_grid),
        sender: crdt.get_sender(),
        transport,
        _driver: driver,
    });

    info!(
        "pulse: session created for {}:{}",
        config.transport.host, config.transport.port
    );

    // TODO(pulse): build + send a HandshakeRequest (auth chain + connect-sig over `server_id`)
    // using the local identity, then a TeleportRequest(realm), gated on PulseStatus::Connected.
}

/// Drain status + inbound bytes each frame; decode and dispatch.
fn pump_pulse(session: Option<ResMut<PulseSession>>) {
    let Some(mut session) = session else {
        return;
    };
    let session = &mut *session;

    while let Ok(status) = session.link.status.try_recv() {
        match status {
            PulseStatus::Connecting => debug!("pulse: connecting"),
            PulseStatus::Connected => info!("pulse: connected"),
            PulseStatus::Disconnected(reason) => warn!("pulse: disconnected ({reason})"),
            PulseStatus::Failed(error) => warn!("pulse: failed ({error})"),
        }
    }

    while let Ok(bytes) = session.link.inbound.try_recv() {
        let events = match pulse::ServerMessage::decode(bytes.as_slice()) {
            Ok(message) => session.decoder.handle(message),
            Err(err) => {
                warn!("pulse: failed to decode ServerMessage: {err}");
                continue;
            }
        };
        for event in events {
            route_event(event, session);
        }
    }
}

/// Forward a decoded event toward its destination. Movement is bridged into the shared
/// foreign-player pipeline as a synthesized `rfc4::Movement` (reusing `update_player` /
/// `foreign_dynamics` verbatim); resync goes back reliably over the driver.
fn route_event(event: PulseEvent, session: &PulseSession) {
    match event {
        PulseEvent::Movement { address, movement } => {
            let update = PlayerUpdate {
                transport_id: session.transport,
                message: PlayerMessage::PlayerData(Message::Movement(*movement)),
                address,
            };
            let _ = session.sender.try_send(update.into());
        }
        PulseEvent::Resync(request) => {
            let message = pulse::ClientMessage {
                message: Some(pulse::client_message::Message::Resync(request)),
            };
            let _ = session.link.outbound.try_send(PulseFrame {
                bytes: message.encode_to_vec(),
                reliability: PulseReliability::Reliable,
            });
        }
        // TODO(pulse): PlayerLeft cleanup of the foreign player; profile-version announce on
        // ProfileVersion; build + send the handshake (and TeleportRequest) on Connected.
        PulseEvent::Joined { .. }
        | PulseEvent::Left { .. }
        | PulseEvent::ProfileVersion { .. }
        | PulseEvent::Connected { .. } => {}
    }
}
