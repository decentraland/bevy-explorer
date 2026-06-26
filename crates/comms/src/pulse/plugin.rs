//! Pulse Bevy plugin — the shared, platform-agnostic protocol layer.
//!
//! Owns the [`PulseDecoder`] and the driver lifecycle, and pumps the byte boundary: inbound
//! `ServerMessage` bytes are decoded and dispatched; outbound `ClientMessage`s (resync today;
//! handshake + input later) are encoded onto the driver. The driver itself (native thread / wasm
//! task) is selected at compile time and never seen here.

use bevy::prelude::*;
use dcl_component::proto_components::pulse;
use prost::Message as _;
use tokio::sync::mpsc::Sender;

use super::transport::{
    self, PulseDriverHandle, PulseFrame, PulseLink, PulseReliability, PulseStatus,
    PulseTransportConfig,
};
use super::{PulseDecoder, PulseEvent, PulseParcelGrid};

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
    /// Held for the session lifetime; dropping it stops the driver.
    _driver: PulseDriverHandle,
}

pub struct PulsePlugin;

impl Plugin for PulsePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (connect_pulse, pump_pulse).chain());
    }
}

/// Bring a session up once a [`PulseConfig`] is present. No-op afterwards (session exists).
fn connect_pulse(
    mut commands: Commands,
    config: Option<Res<PulseConfig>>,
    session: Option<Res<PulseSession>>,
) {
    let (Some(config), None) = (config, session) else {
        return;
    };

    let (link, driver_channels) = transport::pulse_channels(1024);
    let driver = transport::spawn_pulse_driver(config.transport.clone(), driver_channels);

    commands.insert_resource(PulseSession {
        link,
        decoder: PulseDecoder::new(config.parcel_grid),
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
        match pulse::ServerMessage::decode(bytes.as_slice()) {
            Ok(message) => {
                for event in session.decoder.handle(message) {
                    route_event(event, &session.link.outbound);
                }
            }
            Err(err) => warn!("pulse: failed to decode ServerMessage: {err}"),
        }
    }
}

/// Forward a decoded event toward its destination. Resync is wired (reliable ClientMessage back to
/// the server); the foreign-player bridge is the next integration step.
fn route_event(event: PulseEvent, outbound: &Sender<PulseFrame>) {
    match event {
        PulseEvent::Resync(request) => {
            let message = pulse::ClientMessage {
                message: Some(pulse::client_message::Message::Resync(request)),
            };
            let _ = outbound.try_send(PulseFrame {
                bytes: message.encode_to_vec(),
                reliability: PulseReliability::Reliable,
            });
        }
        // TODO(pulse): bridge Movement → PlayerUpdate { transport_id, address, rfc4::Movement }
        // onto GlobalCrdtState's sender; apply Joined/Left to the Pulse transport's foreign_aliases;
        // surface Connected to gate the handshake/teleport.
        PulseEvent::Movement { .. }
        | PulseEvent::Joined { .. }
        | PulseEvent::Left { .. }
        | PulseEvent::ProfileVersion { .. }
        | PulseEvent::Connected { .. } => {}
    }
}
