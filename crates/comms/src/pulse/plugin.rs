//! Pulse Bevy plugin — the shared, platform-agnostic protocol layer.
//!
//! Owns the [`PulseDecoder`] and the driver lifecycle, and pumps the byte boundary: inbound
//! `ServerMessage` bytes are decoded and dispatched; outbound `ClientMessage`s (handshake, teleport,
//! resync today; input later) are encoded onto the driver. The driver itself (native thread / wasm
//! task) is selected at compile time and never seen here.
//!
//! Connect sequence, mirroring the Unity reference client: once the driver reports
//! [`PulseStatus::Connected`] we send a [`pulse::HandshakeRequest`] (a signed auth chain, identical
//! in shape to the platform's `x-identity-*` header dictionary but delivered as protobuf bytes);
//! on the server's `HandshakeResponse` we send the first gameplay message, a
//! [`pulse::TeleportRequest`] announcing our realm + position so the server starts streaming
//! same-realm peers.
//!
//! Reconnection model: a driver runs exactly one connection attempt and, when that attempt ends,
//! drops its channel ends. The protocol layer treats that *pipe close* — not the advisory
//! `Disconnected(reason)` message — as the authoritative "transport is gone" signal, and rebuilds
//! the whole driver/link from `Down` (unless the last reason was terminal, in which case it parks in
//! `Dead`). Initial connect is just the first such build.

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task};
use common::{
    structs::{CurrentRealm, PrimaryUser},
    util::{TaskCompat, TaskExt},
};
use dcl_component::proto_components::common::Vector3;
use dcl_component::proto_components::kernel::comms::rfc4;
use dcl_component::proto_components::pulse;
use dcl_component::transform_and_parent::DclTranslation;
use prost::Message as _;
use tokio::sync::mpsc;
use wallet::Wallet;

use super::transport::{
    self, PulseDriverHandle, PulseFrame, PulseLink, PulseReliability, PulseStatus,
    PulseTransportConfig,
};
use super::{from_movement, PulseDecoder, PulseEvent, PulseParcelGrid};
use crate::global_crdt::{GlobalCrdtState, NetworkUpdate, PlayerMessage, PlayerUpdate};
use crate::{NetworkMessage, Transport, TransportType};

/// Insert this resource to connect to a Pulse server. Absent → the plugin is fully inert.
#[derive(Resource, Clone)]
pub struct PulseConfig {
    pub transport: PulseTransportConfig,
    pub parcel_grid: PulseParcelGrid,
    /// Identifies this server instance; folded into the handshake connect signature. Empty on dev.
    pub server_id: String,
}

/// The Pulse connection's lifecycle — a single linear progression with two off-ramps (`Down` to
/// rebuild, `Dead` to give up). The driver/link are live from `Connecting` through `Established`;
/// `Down` and `Dead` have no driver. There are no side-flags: the transport-up signal *is* the
/// `Connecting → Idle` transition, and a disconnect's reason is consumed the moment it arrives.
enum Connection {
    /// No live driver. The next tick after `respawn_at` (re)builds one. This is both the initial
    /// state and where a retryable transport drop lands.
    Down { respawn_at: f64 },
    /// Driver up, waiting for it to report `Connected` (the ENet/WebTransport connect completing).
    Connecting,
    /// Transport connected, ready to sign once an identity is present and the cooldown elapses.
    /// `retry_after` throttles re-signs after a sign error / rejection / response timeout.
    Idle { retry_after: f64 },
    /// Signing the auth chain off-thread; resolves to the encoded `ClientMessage(handshake)` bytes.
    /// Re-signed on each attempt so the connect-signature timestamp is fresh when sent.
    Signing(Task<Result<Vec<u8>, String>>),
    /// Handshake sent; awaiting the server's `HandshakeResponse` until `timeout_at`.
    AwaitingResponse { timeout_at: f64 },
    /// Handshake accepted and the realm teleport sent; steady state.
    Established,
    /// Terminally disconnected (auth rejected, banned, evicted, flagged) — no reconnect attempted.
    /// Only a fresh session (realm change / re-login recreating `PulseConfig`) clears this.
    Dead,
}

/// Cooldown before re-attempting after any retryable failure (no identity yet, sign error, response
/// timeout, server rejection, or a retryable disconnect), so we don't hammer the wallet or server.
const RETRY_COOLDOWN_SECS: f64 = 2.0;
/// How long to wait for the server's `HandshakeResponse` before assuming it was lost and retrying.
const HANDSHAKE_RESPONSE_TIMEOUT_SECS: f64 = 5.0;

#[derive(Resource)]
pub(crate) struct PulseSession {
    /// The byte boundary to the current driver. `None` between attempts (`Down`/`Dead`).
    link: Option<PulseLink>,
    /// Current driver; dropping it stops the thread. `None` between attempts. Replaced wholesale on
    /// every (re)connect — this is the "machinery" we reinitialise when the pipe closes.
    _driver: Option<PulseDriverHandle>,
    decoder: PulseDecoder,
    /// Sink into the shared foreign-player pipeline — the same channel every transport feeds.
    sender: mpsc::Sender<NetworkUpdate>,
    /// Synthetic transport entity used as the foreign players' `transport_id`.
    foreign_transport: Entity,
    /// World ↔ parcel mapping, used to build our own `TeleportRequest`.
    grid: PulseParcelGrid,
    /// Where to (re)connect — kept so we can rebuild the driver without the `PulseConfig` resource.
    transport_config: PulseTransportConfig,
    /// Server instance id, folded into the connect signature (re-signed on each attempt).
    server_id: String,
    /// Latched true once we first enter a livekit (Pulse) realm. Gates the driver bring-up so we
    /// don't dial out until needed, then stays set so the connection is kept alive across non-Pulse
    /// realms (we simply stop sending to it there — the routing entity is gone).
    wanted: bool,
    state: Connection,
}

impl PulseSession {
    /// Bridge an outbound rfc4 packet — the bytes a `BroadcastTarget::PULSE` send queued onto the
    /// Pulse transport — into a Pulse `ClientMessage`. The write-side mirror of `pump_pulse`'s
    /// decode. No-op unless the session is `Established` with a live link, so callers can fire
    /// unconditionally. Only avatar-state messages have a conversion; anything else is dropped with a
    /// warning (only `PULSE`-targeted sends should ever reach here — see [`BroadcastTarget`]).
    fn send_rfc4(&self, packet: &rfc4::Packet) {
        if !matches!(self.state, Connection::Established) {
            return;
        }
        let Some(link) = self.link.as_ref() else {
            return;
        };
        let message = match &packet.message {
            // Movement → `PlayerStateInput`: the movement is inverted into a Pulse `PlayerState` via
            // [`from_movement`] (the server computes the per-field deltas it fans out) and sent
            // unreliable-sequenced, matching Unity's `PacketMode.UNRELIABLE_SEQUENCED`.
            Some(rfc4::packet::Message::Movement(movement)) => pulse::ClientMessage {
                message: Some(pulse::client_message::Message::Input(
                    pulse::PlayerStateInput {
                        state: Some(from_movement(movement, &self.grid)),
                    },
                )),
            },
            // TODO(pulse): emote → EmoteStart, profile-version → ProfileVersionAnnouncement.
            other => {
                warn!("pulse: no conversion for outbound rfc4 message, dropping: {other:?}");
                return;
            }
        };
        let _ = link.outbound.try_send(PulseFrame {
            bytes: message.encode_to_vec(),
            reliability: PulseReliability::UnreliableSequenced,
        });
    }
}

/// Marker for the synthetic Pulse transport entity used as foreign players' `transport_id`. Distinct
/// from the routing `Transport` entity (which carries [`PulseOutbox`]); this one is never despawned.
#[derive(Component)]
struct PulseTransport;

/// The drain end of the Pulse routing `Transport` entity's channel — its companion, like
/// `WebsocketRoomTransport.receiver`. `drain_pulse_outbox` decodes and bridges what lands here.
#[derive(Component)]
struct PulseOutbox(mpsc::Receiver<NetworkMessage>);

/// Written from `AdapterManager` when a livekit (Pulse) realm is entered: (re)spawn the routing
/// transport, ensure the connection is up, and announce the realm.
#[derive(Event)]
pub struct StartPulse;

pub struct PulsePlugin;

impl Plugin for PulsePlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<StartPulse>();
        app.add_systems(Startup, configure_pulse);
        app.add_systems(
            Update,
            (connect_pulse, start_pulse, pump_pulse, drain_pulse_outbox).chain(),
        );
    }
}

/// Default Pulse endpoint (production). Override with the `PULSE_SERVER=host:port` env var — e.g.
/// `pulse-server.decentraland.zone:7777` for dev, or a local instance.
const DEFAULT_PULSE_SERVER: &str = "pulse-server.decentraland.org:7777";

/// Insert the [`PulseConfig`] that activates the transport. Targets [`DEFAULT_PULSE_SERVER`] unless
/// `PULSE_SERVER` overrides it. The grid is the Decentraland Genesis City `ParcelEncoder` from the
/// server's appsettings ([`PulseParcelGrid::default`]).
fn configure_pulse(mut commands: Commands) {
    let endpoint =
        std::env::var("PULSE_SERVER").unwrap_or_else(|_| DEFAULT_PULSE_SERVER.to_owned());
    let Some((host, port)) = endpoint.rsplit_once(':') else {
        warn!("pulse: PULSE_SERVER must be host:port, got {endpoint:?}");
        return;
    };
    let Ok(port) = port.parse::<u16>() else {
        warn!("pulse: invalid port in PULSE_SERVER={endpoint:?}");
        return;
    };
    commands.insert_resource(PulseConfig {
        transport: PulseTransportConfig {
            host: host.to_owned(),
            port,
        },
        parcel_grid: PulseParcelGrid::default(),
        server_id: String::new(),
    });
    info!("pulse: configured for {endpoint}");
}

/// Build a fresh driver + its protocol-side link for `config`.
fn spawn_driver(config: &PulseTransportConfig) -> (PulseLink, PulseDriverHandle) {
    let (link, channels) = transport::pulse_channels(1024);
    let driver = transport::spawn_pulse_driver(config.clone(), channels);
    (link, driver)
}

/// Bring a session up once a [`PulseConfig`] is present. No-op afterwards (session exists). The
/// driver itself isn't spawned here — the session starts in `Down`, and `pump_pulse` builds it on
/// the first tick, so initial connect and reconnect share one path.
fn connect_pulse(
    mut commands: Commands,
    crdt: Res<GlobalCrdtState>,
    config: Option<Res<PulseConfig>>,
    session: Option<Res<PulseSession>>,
) {
    let (Some(config), None) = (config, session) else {
        return;
    };

    let foreign_transport = commands.spawn(PulseTransport).id();

    commands.insert_resource(PulseSession {
        link: None,
        _driver: None,
        decoder: PulseDecoder::new(config.parcel_grid),
        sender: crdt.get_sender(),
        foreign_transport,
        grid: config.parcel_grid,
        transport_config: config.transport.clone(),
        server_id: config.server_id.clone(),
        wanted: false,
        state: Connection::Down { respawn_at: 0.0 },
    });

    info!(
        "pulse: session created for {}:{}",
        config.transport.host, config.transport.port
    );
}

/// React to a livekit (Pulse) realm being entered: (re)spawn the routing `Transport` entity,
/// mark the session `wanted` (establishing the connection on the first realm), and — if already
/// connected — announce the new realm with a teleport. The previous routing entity has been
/// despawned by `process_realm_change` this same frame, so we spawn unconditionally (once per frame
/// with an event) rather than presence-checking, which would race that deferred despawn.
fn start_pulse(
    mut commands: Commands,
    mut events: EventReader<StartPulse>,
    session: Option<ResMut<PulseSession>>,
    realm: Res<CurrentRealm>,
    player: Query<&GlobalTransform, With<PrimaryUser>>,
) {
    if events.is_empty() {
        return;
    }
    events.clear();
    let Some(mut session) = session else {
        // `PulseConfig` absent, or `connect_pulse`'s deferred insert hasn't applied yet. A realm
        // change re-fires `StartPulse`, so a missed first event self-heals on the next one.
        return;
    };

    let (sender, receiver) = mpsc::channel(1000);
    commands.spawn((
        Transport {
            transport_type: TransportType::Pulse,
            sender,
            control: None,
            foreign_aliases: Default::default(),
        },
        PulseOutbox(receiver),
    ));

    session.wanted = true;
    // Already up (a later realm) → re-teleport now. Otherwise the first handshake's
    // `on_handshake_response` sends the initial teleport once the connection establishes.
    if matches!(session.state, Connection::Established) {
        send_teleport(&session, &realm, &player);
    }
}

/// Drain the Pulse routing entity's channel each frame and bridge the rfc4 bytes a `PULSE`-targeted
/// broadcast queued there into Pulse `ClientMessage`s. No-op until the session connects.
fn drain_pulse_outbox(session: Option<Res<PulseSession>>, mut outboxes: Query<&mut PulseOutbox>) {
    let Some(session) = session else {
        return;
    };
    for mut outbox in outboxes.iter_mut() {
        while let Ok(message) = outbox.0.try_recv() {
            match rfc4::Packet::decode(message.data.as_slice()) {
                Ok(packet) => session.send_rfc4(&packet),
                Err(err) => warn!("pulse: failed to decode outbound rfc4 packet: {err}"),
            }
        }
    }
}

/// Drain status + inbound bytes each frame; advance the connection; decode and dispatch.
fn pump_pulse(
    session: Option<ResMut<PulseSession>>,
    realm: Res<CurrentRealm>,
    wallet: Res<Wallet>,
    time: Res<Time>,
    player: Query<&GlobalTransform, With<PrimaryUser>>,
) {
    let Some(session) = session else {
        return;
    };
    let session = session.into_inner();
    let now = time.elapsed_secs_f64();

    drain_status(session, now);
    drive_connection(session, &wallet, now);
    drain_inbound(session, &realm, &player, now);
}

/// Drain the driver's status channel into the connection state machine. `link`'s borrow ends at
/// `try_recv`, so the arms are free to mutate `session`. A `Disconnected`/`Failed` status is the
/// driver signing off, handled as the teardown; a bare channel close means it vanished with no word
/// (e.g. panicked) — the fallback teardown. Either way `lost_connection` nulls the link, ending the
/// loop and making a status-then-close sequence idempotent.
fn drain_status(session: &mut PulseSession, now: f64) {
    while let Some(status) = session.link.as_mut().map(|link| link.status.try_recv()) {
        match status {
            Ok(PulseStatus::Connecting) => debug!("pulse: connecting"),
            Ok(PulseStatus::Connected) => {
                info!("pulse: connected");
                if matches!(session.state, Connection::Connecting) {
                    session.state = Connection::Idle { retry_after: 0.0 };
                }
            }
            Ok(PulseStatus::Disconnected(reason)) => {
                warn!("pulse: disconnected ({reason:?})");
                lost_connection(session, reason.should_retry(), now);
            }
            // Never established (DNS/socket/connect timeout) — always transient.
            Ok(PulseStatus::Failed(error)) => {
                warn!("pulse: failed ({error})");
                lost_connection(session, true, now);
            }
            Err(mpsc::error::TryRecvError::Empty) => break,
            Err(mpsc::error::TryRecvError::Disconnected) => {
                lost_connection(session, true, now);
                break;
            }
        }
    }
}

/// Advance the connection one step. `Down` (re)builds the driver; `Connecting` waits passively for
/// the transport-up status; signing waits additionally for an identity (`PulseConfig` may be present
/// before login). Each retryable handshake failure folds back to `Idle` (driver still up) with a
/// cooldown.
fn drive_connection(session: &mut PulseSession, wallet: &Wallet, now: f64) {
    match &mut session.state {
        // Passive states: `Connecting` waits for the transport-up status; the others are terminal or
        // steady.
        Connection::Dead | Connection::Established | Connection::Connecting => {}
        Connection::Down { respawn_at } => {
            // Don't dial out until a livekit realm has been entered (`start_pulse` sets `wanted`).
            if !session.wanted || now < *respawn_at {
                return;
            }
            let (link, driver) = spawn_driver(&session.transport_config);
            session.link = Some(link);
            session._driver = Some(driver);
            session.state = Connection::Connecting;
        }
        Connection::Idle { retry_after } => {
            // Hold off until the cooldown elapses and we have an identity to sign with.
            if now < *retry_after || wallet.address().is_none() {
                return;
            }
            let wallet = wallet.clone();
            let server_id = session.server_id.clone();
            let task = IoTaskPool::get().spawn_compat(async move {
                build_handshake_request(&wallet, &server_id)
                    .await
                    .map(|request| {
                        pulse::ClientMessage {
                            message: Some(pulse::client_message::Message::Handshake(request)),
                        }
                        .encode_to_vec()
                    })
            });
            session.state = Connection::Signing(task);
        }
        Connection::Signing(task) => {
            if let Some(result) = task.complete() {
                match result {
                    Ok(bytes) => {
                        if let Some(link) = session.link.as_ref() {
                            let _ = link.outbound.try_send(PulseFrame {
                                bytes,
                                reliability: PulseReliability::Reliable,
                            });
                        }
                        info!("pulse: handshake sent");
                        session.state = Connection::AwaitingResponse {
                            timeout_at: now + HANDSHAKE_RESPONSE_TIMEOUT_SECS,
                        };
                    }
                    Err(err) => {
                        warn!("pulse: failed to build handshake, retrying: {err}");
                        session.state = Connection::Idle {
                            retry_after: now + RETRY_COOLDOWN_SECS,
                        };
                    }
                }
            }
        }
        Connection::AwaitingResponse { timeout_at } => {
            if now > *timeout_at {
                warn!("pulse: handshake response timed out, retrying");
                session.state = Connection::Idle {
                    retry_after: now + RETRY_COOLDOWN_SECS,
                };
            }
        }
    }
}

/// Decode + route inbound `ServerMessage` bytes. Same borrow trick: `link` is released at `try_recv`,
/// so the body can drive the decoder and event handlers through `session`.
fn drain_inbound(
    session: &mut PulseSession,
    realm: &CurrentRealm,
    player: &Query<&GlobalTransform, With<PrimaryUser>>,
    now: f64,
) {
    while let Some(Ok(bytes)) = session.link.as_mut().map(|link| link.inbound.try_recv()) {
        let events = match pulse::ServerMessage::decode(bytes.as_slice()) {
            Ok(message) => session.decoder.handle(message),
            Err(err) => {
                warn!("pulse: failed to decode ServerMessage: {err}");
                continue;
            }
        };
        for event in events {
            match event {
                // The handshake ack drives the connect sequence.
                PulseEvent::Connected { success, error } => {
                    on_handshake_response(session, realm, player, now, success, error)
                }
                // Movement is bridged into the shared foreign-player pipeline as its own
                // `PlayerMessage::Movement`, reusing `update_player` / `foreign_dynamics` verbatim.
                PulseEvent::Movement { address, movement } => {
                    let update = PlayerUpdate {
                        transport_id: session.foreign_transport,
                        message: PlayerMessage::Movement(movement),
                        address,
                    };
                    let _ = session.sender.try_send(update.into());
                }
                // A sequence gap was detected — ask the server to replay full state, reliably.
                PulseEvent::Resync(request) => {
                    let message = pulse::ClientMessage {
                        message: Some(pulse::client_message::Message::Resync(request)),
                    };
                    if let Some(link) = session.link.as_ref() {
                        let _ = link.outbound.try_send(PulseFrame {
                            bytes: message.encode_to_vec(),
                            reliability: PulseReliability::Reliable,
                        });
                    }
                }
                // TODO(pulse): PlayerLeft cleanup of the foreign player; profile-version announce.
                PulseEvent::Joined { .. }
                | PulseEvent::Left { .. }
                | PulseEvent::ProfileVersion { .. } => {}
            }
        }
    }
}

/// The transport is gone — tear the driver/link down and decide what's next: schedule a rebuild from
/// `Down` after a cooldown, or, for a terminal reason, park in `Dead`. Idempotent: with no live link
/// it's a no-op, so a `Disconnected` status followed by the pipe close only acts once.
fn lost_connection(session: &mut PulseSession, retry: bool, now: f64) {
    if session.link.is_none() {
        return;
    }
    session.link = None;
    session._driver = None; // dropping joins the already-exited driver thread
    session.state = if retry {
        info!("pulse: transport dropped — reinitialising after cooldown");
        Connection::Down {
            respawn_at: now + RETRY_COOLDOWN_SECS,
        }
    } else {
        warn!("pulse: terminal disconnect — not reconnecting");
        Connection::Dead
    };
}

/// Handle the server's `HandshakeResponse`. On success, send the first gameplay message — a
/// `TeleportRequest` announcing our realm + position, so the server begins streaming same-realm
/// peers (peers in different realms never see each other).
fn on_handshake_response(
    session: &mut PulseSession,
    realm: &CurrentRealm,
    player: &Query<&GlobalTransform, With<PrimaryUser>>,
    now: f64,
    success: bool,
    error: Option<String>,
) {
    // Ignore a stray response we're not waiting on (e.g. a duplicate after we've established).
    if !matches!(session.state, Connection::AwaitingResponse { .. }) {
        return;
    }
    if !success {
        warn!(
            "pulse: handshake rejected, retrying: {}",
            error.unwrap_or_default()
        );
        session.state = Connection::Idle {
            retry_after: now + RETRY_COOLDOWN_SECS,
        };
        return;
    }
    info!("pulse: handshake accepted");
    session.state = Connection::Established;
    send_teleport(session, realm, player);
}

/// Send a `TeleportRequest` announcing our current realm + position, so the server (re)starts
/// streaming same-realm peers (peers in different realms never see each other). The `realm` string is
/// the load-bearing field; a one-frame stale position is corrected by the next movement packet. Sent
/// reliably. No-op without a live link or realm name. Used both on first handshake and on every later
/// realm change (the server supports same-peer re-teleports).
fn send_teleport(
    session: &PulseSession,
    realm: &CurrentRealm,
    player: &Query<&GlobalTransform, With<PrimaryUser>>,
) {
    let Some(link) = session.link.as_ref() else {
        return;
    };

    // The realm identifier must match what other clients announce (Unity sends
    // `configurations.realmName`). Empty/missing realm is rejected by the server.
    let Some(realm_name) = realm
        .config
        .realm_name
        .clone()
        .filter(|name| !name.is_empty())
    else {
        warn!("pulse: no realm name yet; skipping teleport (no peers will be visible)");
        return;
    };

    // Bevy render position → DCL world coords (the `-z` flip), then split into parcel + local —
    // exactly the inverse of how inbound state is decoded.
    let world = player
        .single()
        .map(|t| t.translation())
        .unwrap_or(Vec3::ZERO);
    let dcl = DclTranslation::from_bevy_translation(world).0;
    let (parcel_index, local) = session
        .grid
        .encode_to_parcel(Vec3::new(dcl[0], dcl[1], dcl[2]));

    let teleport = pulse::TeleportRequest {
        parcel_index,
        position: Some(Vector3 {
            x: local.x,
            y: local.y,
            z: local.z,
        }),
        realm: realm_name,
    };
    let message = pulse::ClientMessage {
        message: Some(pulse::client_message::Message::Teleport(teleport)),
    };
    let _ = link.outbound.try_send(PulseFrame {
        bytes: message.encode_to_vec(),
        reliability: PulseReliability::Reliable,
    });
    info!("pulse: teleport sent (parcel {parcel_index})");
}

/// Build a `HandshakeRequest`: sign `connect:/{server_id}:{ts}:{}` with the local identity and pack
/// the resulting auth chain into the platform's canonical `x-identity-*` dictionary (JSON object,
/// every value a string), serialized as UTF-8 bytes — identical in shape to the HTTP signed-fetch
/// headers, just carried in a protobuf `bytes` field. Mirrors Unity's `BuildAuthChain`.
async fn build_handshake_request(
    wallet: &Wallet,
    server_id: &str,
) -> Result<pulse::HandshakeRequest, String> {
    let timestamp = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();

    // NB: not lowercased — the server verifies the signature over this exact string, and Unity
    // signs it verbatim.
    let payload = format!("connect:/{server_id}:{timestamp}:{{}}");
    let auth_chain = wallet
        .sign_message(payload)
        .await
        .map_err(|e| e.to_string())?;

    let mut dict = serde_json::Map::new();
    for (key, value) in auth_chain.headers() {
        dict.insert(key, serde_json::Value::String(value));
    }
    dict.insert(
        "x-identity-timestamp".to_owned(),
        serde_json::Value::String(timestamp.to_string()),
    );
    dict.insert(
        "x-identity-metadata".to_owned(),
        serde_json::Value::String("{}".to_owned()),
    );
    let auth_chain = serde_json::to_vec(&dict).map_err(|e| e.to_string())?;

    Ok(pulse::HandshakeRequest {
        auth_chain,
        // TODO(pulse): announce the real profile version (Unity sends the self-profile version).
        profile_version: 0,
        initial_state: None,
    })
}
