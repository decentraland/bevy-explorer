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

use std::sync::{Arc, Weak};

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task};
use common::{
    bounds_calc::scene_regions,
    structs::{CurrentRealm, OutOfWorld, PlayerTeleported, PrimaryUser},
    util::{TaskCompat, TaskExt},
};
use dcl_component::proto_components::kernel::comms::rfc4;
use dcl_component::proto_components::pulse;
use dcl_component::transform_and_parent::DclTranslation;
use ethers_core::types::Address;
use ipfs::{machine_id_from_b64, EntityDefinition};
use prost::Message as _;
use tokio::sync::mpsc;
use wallet::Wallet;

use super::transport::{
    self, PulseDriverHandle, PulseFrame, PulseLink, PulseReliability, PulseStatus,
    PulseTransportConfig,
};
use super::{PulseCtx, PulseDecoder, PulseEvent, PulseParcelGrid};
use crate::global_crdt::{
    CrdtContexts, GlobalCrdtState, NetworkUpdate, PlayerMessage, PlayerUpdate, SceneRealms,
};
use crate::profile::CurrentUserProfile;
use crate::{NetworkMessage, Transport, TransportType};
use bevy::platform::collections::HashMap;

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
    /// The crdt context this session feeds on a client: Pulse is the *realm's* avatar-state
    /// transport, so that is always the shared context. Unused by a listening server, which routes
    /// per context instead — see [`ListenerAoi`].
    context: Entity,
    /// Sink into `context`'s foreign-player pipeline — the same channel every transport feeds.
    sender: mpsc::Sender<NetworkUpdate>,
    /// The realm's routing `Transport` entity, which doubles as the `transport_id` every inbound
    /// Pulse update is attributed to. `None` off a Pulse realm (the entity is despawned with the
    /// realm's transports), which is also when inbound is gated off by the liveness anchor.
    ///
    /// Using the real transport entity — rather than a synthetic marker — is what makes presence
    /// work: `ForeignPlayer.transports` holds it, so despawning it on a realm change drops every
    /// Pulse peer from their presence set, exactly as it does for LiveKit and ws-room.
    routing_transport: Option<Entity>,
    /// The realm `routing_transport` was spawned for. `StartPulse` also fires for archipelago
    /// island hops within one realm, and those must NOT rebuild the transport — see `start_pulse`.
    routing_realm: Option<String>,
    /// The dev server's machine id on a locally served realm. Announced as
    /// `{realm_name}-{machine_id}-{port}` so previews don't share a Pulse partition — every dev
    /// server advertises the same `LocalPreview` realm name, the machine id separates two devs, and
    /// the port separates two servers on one machine. `None` off a local realm, and until a `b64-`
    /// addressed scene has loaded — `resolve_realm_qualifier` fills it in and re-announces.
    realm_qualifier: Option<String>,
    /// World ↔ parcel mapping, used to build our own `TeleportRequest`.
    grid: PulseParcelGrid,
    /// Where to (re)connect — kept so we can rebuild the driver without the `PulseConfig` resource.
    transport_config: PulseTransportConfig,
    /// Server instance id, folded into the connect signature (re-signed on each attempt).
    server_id: String,
    /// Latched true once we first enter a Pulse realm. Gates the driver bring-up so we
    /// don't dial out until needed, then stays set so the connection is kept alive across non-Pulse
    /// realms (we simply stop sending to it there — the routing entity is gone).
    wanted: bool,
    /// Last `PlayerState` we sent, cached by movement's `Broadcast::to_pulse`. An outbound
    /// `EmoteStart` attaches this — the server rejects an emote with a null `player_state`.
    last_state: Option<pulse::PlayerState>,
    /// Set on an authoritative server: the engine joins Pulse as a receive-only *scene listener*
    /// over a parcel-rect AoI instead of as a subject, and demuxes what it receives into the
    /// per-scene crdt contexts. `None` on a client.
    listener: Option<ListenerAoi>,
    /// A listener's own strong clone of [`PulseSession::liveness`]. A listener's connection is
    /// governed by its AoI, not by any one transport entity — its per-context transports come and go
    /// with the scenes — so the anchor lives with the session for as long as the session does,
    /// rather than with a routing entity it doesn't have.
    #[expect(dead_code)]
    listener_anchor: Option<Arc<()>>,
    /// Liveness anchor for the realm's Pulse routing entity. The routing entity holds a strong clone
    /// (`PulsePresence`) while it exists; the driver holds a `Weak` and surfaces inbound only while
    /// `strong_count() > 1`. Lives here (not on the entity) so reconnects, which rebuild the driver,
    /// can hand it a fresh `Weak` — the entity (and thus the signal) outlives any single driver.
    liveness: Arc<()>,
    state: Connection,
}

/// A scene listener's view of the world it hosts: which crdt context owns each parcel, the AoI those
/// parcels add up to, and the per-context plumbing inbound state is routed through.
///
/// The AoI is announced in the handshake and reassigned afterwards with `SceneListenerUpdate`, the
/// one post-auth message a listener may send besides `Resync`. The server swaps the set in place, so
/// the connection, identity and realm all survive a change — see [`set_listener_aoi`].
#[derive(Default)]
struct ListenerAoi {
    /// Realm → hosted parcel → the crdt context whose scene covers it. Pulse reports *where* a peer
    /// is; this is what makes that *whose scene* it is in. Keyed by realm as well as parcel because
    /// a parcel index only means something inside one: every world numbers its parcels from 0,0, so
    /// a server cohosting two worlds has two different scenes at `0,0`. Also the change key:
    /// rebuilding everything below is driven off this map differing from the last frame's.
    context_by_parcel: HashMap<String, HashMap<IVec2, Entity>>,
    /// Per crdt context: the routing `Transport` its Pulse state is attributed to, and its inbound
    /// channel.
    contexts: HashMap<Entity, ListenerContext>,
    /// Announced AoI: one entry per hosted realm, its parcels consolidated into one rect per
    /// contiguous rectangular block ([`scene_regions`]). The rects tile each realm's parcel set
    /// exactly — no overlap, no over-coverage — so consolidating them is purely a wire-size win
    /// against the server's Σ-area budget.
    aoi: Vec<pulse::SceneListenerAoi>,
    /// The context each peer was last routed to, so moving between scenes reports a departure from
    /// the one it left. Pulse gives us positions, not scene membership; this turns the one into the
    /// other.
    peer_context: HashMap<Address, Entity>,
}

/// One crdt context's end of a listener's routing: the `Transport` entity that owns the context's
/// Pulse presence and the channel its updates go down.
struct ListenerContext {
    transport: Entity,
    sender: mpsc::Sender<NetworkUpdate>,
}

impl PulseSession {
    /// Forward an inbound Pulse update for a peer standing in `parcel` of `realm`, re-homing it
    /// first if that lands in a different scene than the one it was last routed to. A peer that has
    /// left every hosted parcel is reported as departed and its state dropped.
    ///
    /// On a client the position is irrelevant — the realm is the AoI — so this is just
    /// [`Self::forward`].
    fn forward_at(&mut self, address: Address, realm: &str, parcel: IVec2, message: PlayerMessage) {
        if let Some(listener) = self.listener.as_mut() {
            let context = listener
                .context_by_parcel
                .get(realm)
                .and_then(|parcels| parcels.get(&parcel))
                .copied();

            // Moving between scenes (or out of all of them) is a departure from the previous one:
            // Pulse presence is per context, so the context it left has to be told.
            if let Some(previous) = listener.peer_context.get(&address).copied() {
                if Some(previous) != context {
                    if let Some(slot) = listener.contexts.get(&previous) {
                        let _ = slot.sender.try_send(NetworkUpdate::PlayerLeft {
                            transport_id: slot.transport,
                            address,
                        });
                    }
                    listener.peer_context.remove(&address);
                }
            }

            let Some(context) = context else {
                return;
            };
            listener.peer_context.insert(address, context);
        }

        self.forward(address, message);
    }

    /// Forward an inbound Pulse update to whoever should see it.
    ///
    /// On a client that is this session's crdt context, attributed to the realm's routing transport;
    /// dropped when there is no routing transport — we're off a Pulse realm, and a straggling packet
    /// must not resurrect a peer whose transport is already gone.
    ///
    /// On a listening server it is the context the peer was last placed in by [`Self::forward_at`],
    /// attributed to that context's own Pulse transport. Events that carry no position (emotes,
    /// profile versions) ride the placement the peer's last movement established; a peer we have
    /// never placed is not in any of our scenes, so its state is dropped.
    fn forward(&self, address: Address, message: PlayerMessage) {
        let (sender, transport_id) = match self.listener.as_ref() {
            Some(listener) => {
                let Some(slot) = listener
                    .peer_context
                    .get(&address)
                    .and_then(|context| listener.contexts.get(context))
                else {
                    return;
                };
                (&slot.sender, slot.transport)
            }
            None => {
                let Some(transport_id) = self.routing_transport else {
                    return;
                };
                (&self.sender, transport_id)
            }
        };

        let _ = sender.try_send(
            PlayerUpdate {
                transport_id,
                message,
                address,
            }
            .into(),
        );
    }

    /// Report a peer's departure from wherever it currently is, and forget it. Used for a Pulse
    /// `Left` (it fell out of the AoI, or disconnected) — the peer stops being a member of the
    /// context's Pulse transport, and once no transport is left it is despawned.
    fn forget(&mut self, address: Address) {
        let transport_id = match self.listener.as_mut() {
            Some(listener) => {
                let Some(slot) = listener
                    .peer_context
                    .remove(&address)
                    .and_then(|context| listener.contexts.get(&context))
                else {
                    return;
                };
                let _ = slot.sender.try_send(NetworkUpdate::PlayerLeft {
                    transport_id: slot.transport,
                    address,
                });
                return;
            }
            None => self.routing_transport,
        };

        if let Some(transport_id) = transport_id {
            let _ = self.sender.try_send(NetworkUpdate::PlayerLeft {
                transport_id,
                address,
            });
        }
    }
}

/// The realm's Pulse routing entity holds this while it exists (i.e. while we're on a Pulse realm).
/// It's a strong clone of [`PulseSession::liveness`]; despawning the entity drops it, which the driver
/// observes via its `Weak` to stop surfacing inbound peer state. See [`PulseSession::liveness`].
#[derive(Component)]
struct PulsePresence(#[expect(dead_code)] Arc<()>);

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
        app.add_event::<PlayerTeleported>();
        app.add_systems(Startup, configure_pulse);
        app.add_systems(
            Update,
            (
                connect_pulse,
                update_listener_aoi,
                // ahead of `start_pulse`: a realm change clears the qualifier here, so the
                // re-announce that `start_pulse` may send can't carry the previous realm's one.
                resolve_realm_qualifier,
                start_pulse,
                pump_pulse,
                drain_pulse_outbox,
            )
                .chain(),
        );
        app.add_systems(Update, pulse_teleport_on_local_move);
    }
}

/// Default Pulse endpoint (production). Native speaks ENet/UDP (7777); wasm has no ENet, so it speaks
/// WebTransport on 7743. Override with the `PULSE_SERVER=host:port` env var — e.g.
/// `pulse-server.decentraland.zone` for dev, or a local instance.
#[cfg(not(target_arch = "wasm32"))]
const DEFAULT_PULSE_SERVER: &str = "pulse-server.decentraland.org:7777";
#[cfg(target_arch = "wasm32")]
const DEFAULT_PULSE_SERVER: &str = "pulse-server.decentraland.org:7743";

/// SHA-256 to pin the server's TLS cert via WebTransport's `serverCertificateHashes`. Production is
/// CA-signed → `None` (default trust); native (ENet) never needs one.
#[cfg(not(target_arch = "wasm32"))]
fn dev_cert_hash() -> Option<Vec<u8>> {
    None
}
#[cfg(target_arch = "wasm32")]
fn dev_cert_hash() -> Option<Vec<u8>> {
    // To test against a local self-signed dev server, point DEFAULT_PULSE_SERVER at it and return the
    // SHA-256 of its cert (e.g. claude-work/pulse-dev-cert/cert.pem — regenerate + refresh if expired,
    // `openssl x509 -outform DER | openssl dgst -sha256`):
    // Some(vec![
    //     0x00, 0x0c, 0x4f, 0xec, 0xc3, 0x81, 0x1d, 0xe4, 0x9a, 0x8a, 0x9d, 0x31, 0x6c, 0x3e, 0x40,
    //     0x49, 0xdd, 0xb4, 0x8e, 0x0f, 0xfd, 0x87, 0x51, 0x60, 0x92, 0x01, 0x35, 0xb4, 0x1a, 0xf7,
    //     0xdd, 0xda,
    // ])
    None
}

/// Insert the [`PulseConfig`] that activates the transport, on clients and servers alike — a server
/// joins as a scene listener rather than a subject (see [`ListenerAoi`]). Targets
/// [`DEFAULT_PULSE_SERVER`] unless `PULSE_SERVER` overrides it. The grid is the Decentraland Genesis City `ParcelEncoder` from the
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
            cert_hash: dev_cert_hash(),
        },
        parcel_grid: PulseParcelGrid::default(),
        server_id: String::new(),
    });
    info!("pulse: configured for {endpoint}");
}

/// Build a fresh driver + its protocol-side link for `config`. `presence` is a weak handle to the
/// session's liveness anchor, handed fresh to every (re)built driver so it survives reconnects.
fn spawn_driver(
    config: &PulseTransportConfig,
    presence: Weak<()>,
) -> (PulseLink, PulseDriverHandle) {
    let (link, channels) = transport::pulse_channels(1024, presence);
    let driver = transport::spawn_pulse_driver(config.clone(), channels);
    (link, driver)
}

/// Bring a session up once a [`PulseConfig`] is present. No-op afterwards (session exists). The
/// driver itself isn't spawned here — the session starts in `Down`, and `pump_pulse` builds it on
/// the first tick, so initial connect and reconnect share one path.
fn connect_pulse(
    mut commands: Commands,
    contexts: Res<CrdtContexts>,
    crdt: Query<&GlobalCrdtState>,
    config: Option<Res<PulseConfig>>,
    session: Option<Res<PulseSession>>,
) {
    let (Some(config), None) = (config, session) else {
        return;
    };

    // client-only (`configure_pulse` bails in server mode): the realm's avatar state feeds the
    // single shared context, which is spawned with the plugin and outlives every session.
    let context = contexts.shared();
    let Ok(crdt) = crdt.get(context) else {
        return;
    };

    let liveness = Arc::new(());
    let listener = common::structs::server_mode().then(ListenerAoi::default);

    commands.insert_resource(PulseSession {
        link: None,
        _driver: None,
        decoder: PulseDecoder::new(config.parcel_grid),
        context,
        sender: crdt.get_sender(),
        routing_transport: None,
        routing_realm: None,
        realm_qualifier: None,
        // A listener never spawns a routing transport, so nothing else would hold the anchor and the
        // driver would drop every inbound frame — the handshake response included.
        listener_anchor: listener.is_some().then(|| liveness.clone()),
        listener,
        grid: config.parcel_grid,
        transport_config: config.transport.clone(),
        server_id: config.server_id.clone(),
        wanted: false,
        last_state: None,
        liveness,
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
    out_of_world: Query<(), (With<PrimaryUser>, With<OutOfWorld>)>,
    routing: Query<(), With<PulseOutbox>>,
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

    // `StartPulse` fires whenever a livekit realm island connects — which includes an archipelago
    // island hop *within* the same realm. Only a realm change sweeps the transports
    // (`process_realm_change` despawns everything `With<Transport>`), so only then is a new routing
    // entity needed.
    //
    // Rebuilding it on an island hop would be actively harmful. Pulse peers are held in
    // `ForeignPlayer.transports` by this entity alone, and a player with an empty set is despawned
    // on the spot now that the inactivity grace period is gone — so an island hop would despawn and
    // respawn every peer, firing `playerDisconnected`/`onLeaveScene` for people who never left. Peers
    // riding Pulse are precisely the ones who should survive the hop: unlike a livekit island, their
    // transport does not change. Leaving it alone also avoids re-teleporting for a realm that hasn't
    // changed, and the churn of a fresh channel per hop.
    let realm_changed = session.routing_realm.as_deref() != Some(realm.address.as_str());
    // Confirmed against the world rather than trusted from the id alone. Nothing sweeps transports
    // while the realm is unchanged, so this particular check cannot race a deferred despawn — and if
    // the entity did go away for some other reason, falling through rebuilds it instead of leaving
    // Pulse silently holding a dead id with nothing to send on.
    let routing_alive = session
        .routing_transport
        .is_some_and(|entity| routing.contains(entity));
    if !realm_changed && routing_alive {
        return;
    }

    // A realm change: the sweep above already queued our old entity for despawn, but that is a
    // deferred command we cannot observe yet — so drop the one we own explicitly (idempotent) and
    // spawn fresh, rather than presence-checking and racing the flush.
    if let Some(previous) = session.routing_transport.take() {
        if let Ok(mut entity) = commands.get_entity(previous) {
            entity.despawn();
        }
    }

    let (sender, receiver) = mpsc::channel(1000);
    let routing_transport = commands
        .spawn((
            Transport {
                transport_type: TransportType::Pulse,
                sender,
                control: None,
                context: session.context,
            },
            PulseOutbox(receiver),
            // While this entity lives (i.e. we're on a Pulse realm) the driver sees a strong ref and
            // surfaces inbound peer state; despawn (realm change away from Pulse) drops it.
            PulsePresence(session.liveness.clone()),
        ))
        .id();

    session.routing_transport = Some(routing_transport);
    session.routing_realm = Some(realm.address.clone());
    session.wanted = true;
    // Already up (a later realm) → re-teleport now, unless out of world (position provisional behind
    // the loading screen); the spawn `PlayerTeleported` re-announces realm + position. Otherwise the
    // first handshake's `on_handshake_response` sends the initial teleport once established.
    if matches!(session.state, Connection::Established) && out_of_world.is_empty() {
        send_teleport(&session, &realm, &player);
    }
}

/// Drain the Pulse routing entity's channel each frame and convert each queued message to a Pulse
/// frame via [`Broadcast::to_pulse`] — movement → `PlayerStateInput`, emote → `EmoteStart`/`EmoteStop`
/// — sending what comes back. Messages with no Pulse form (e.g. byte-only chat/profile that happened
/// onto this transport) yield `None` and are dropped. No-op until the session is `Established`.
fn drain_pulse_outbox(
    session: Option<ResMut<PulseSession>>,
    mut outboxes: Query<&mut PulseOutbox>,
) {
    let Some(session) = session else {
        return;
    };
    let session = session.into_inner();
    let established = matches!(session.state, Connection::Established) && session.link.is_some();
    let grid = session.grid;
    for mut outbox in outboxes.iter_mut() {
        while let Ok(message) = outbox.0.try_recv() {
            if !established {
                continue;
            }
            let frame = {
                let mut ctx = PulseCtx {
                    grid: &grid,
                    last_state: &mut session.last_state,
                };
                message.message.to_pulse(&mut ctx)
            };
            if let (Some(frame), Some(link)) = (frame, session.link.as_ref()) {
                let _ = link.outbound.try_send(frame);
            }
        }
    }
}

/// Drain status + inbound bytes each frame; advance the connection; decode and dispatch.
#[allow(clippy::too_many_arguments)]
fn pump_pulse(
    session: Option<ResMut<PulseSession>>,
    realm: Res<CurrentRealm>,
    wallet: Res<Wallet>,
    time: Res<Time>,
    player: Query<&GlobalTransform, With<PrimaryUser>>,
    profile: Option<Res<CurrentUserProfile>>,
    out_of_world: Query<(), (With<PrimaryUser>, With<OutOfWorld>)>,
) {
    let Some(session) = session else {
        return;
    };
    let session = session.into_inner();
    let now = time.elapsed_secs_f64();
    let in_world = out_of_world.is_empty();

    // Version to announce in the handshake — our connect-time profile announce on Pulse. 0 until the
    // profile loads; the periodic announce (`profile::mod`) corrects it once available.
    let profile_version = profile
        .and_then(|p| p.profile.as_ref().map(|p| p.version as i32))
        .unwrap_or(0);

    drain_status(session, now);
    drive_connection(session, &wallet, profile_version, now);
    drain_inbound(session, &realm, &player, in_world, now);
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
fn drive_connection(session: &mut PulseSession, wallet: &Wallet, profile_version: i32, now: f64) {
    match &mut session.state {
        // Passive states: `Connecting` waits for the transport-up status; the others are terminal or
        // steady.
        Connection::Dead | Connection::Established | Connection::Connecting => {}
        Connection::Down { respawn_at } => {
            // Don't dial out until a livekit realm has been entered (`start_pulse` sets `wanted`).
            if !session.wanted || now < *respawn_at {
                return;
            }
            let (link, driver) =
                spawn_driver(&session.transport_config, Arc::downgrade(&session.liveness));
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
            let task = if let Some(listener) = session.listener.as_ref() {
                // A listener announces its AoI up front and is never a subject: no initial state,
                // no profile version, no teleport. Nothing to observe yet means nothing to announce
                // — stay idle rather than burn a connection on an empty AoI. The AoI carries its
                // own realms, one per hosted world, so there is no connection-wide realm to resolve.
                if listener.aoi.is_empty() {
                    return;
                }
                let aoi = listener.aoi.clone();
                IoTaskPool::get().spawn_compat(async move {
                    build_listener_handshake_request(&wallet, &server_id, aoi)
                        .await
                        .map(|request| {
                            pulse::ClientMessage {
                                message: Some(
                                    pulse::client_message::Message::SceneListenerHandshake(request),
                                ),
                            }
                            .encode_to_vec()
                        })
                })
            } else {
                IoTaskPool::get().spawn_compat(async move {
                    build_handshake_request(&wallet, &server_id, profile_version)
                        .await
                        .map(|request| {
                            pulse::ClientMessage {
                                message: Some(pulse::client_message::Message::Handshake(request)),
                            }
                            .encode_to_vec()
                        })
                })
            };
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
                        match session.listener.as_ref() {
                            Some(listener) => info!(
                                "pulse: scene-listener handshake sent ({})",
                                describe_aoi(&listener.aoi)
                            ),
                            None => info!("pulse: handshake sent"),
                        }
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
    in_world: bool,
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
                    on_handshake_response(session, realm, player, in_world, now, success, error)
                }
                // Movement is bridged into the shared foreign-player pipeline as its own
                // `PlayerMessage::Movement`, reusing `update_player` / `foreign_dynamics` verbatim.
                PulseEvent::Movement {
                    address,
                    movement,
                    realm,
                    teleport,
                    timestamp,
                } => {
                    // Realm + position is the placement: together they decide which of the server's
                    // scenes the peer is in, and so which context sees this and everything after it.
                    let parcel = session.grid.parcel_coords(Vec3::new(
                        movement.position_x,
                        movement.position_y,
                        movement.position_z,
                    ));
                    session.forward_at(
                        address,
                        &realm,
                        parcel,
                        PlayerMessage::Movement {
                            movement,
                            teleport,
                            timestamp,
                        },
                    )
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
                // Emote start/stop are delivered natively (`PlayerMessage::Emote`) rather than as an
                // rfc4 `PlayerEmote`: byte-transport emotes are dropped as duplicates, so the Pulse
                // copy has to be distinguishable from them by variant, exactly as movement is.
                PulseEvent::EmoteStart { address, urn, tick } => session.forward(
                    address,
                    PlayerMessage::Emote {
                        urn,
                        incremental_id: tick,
                        stopping: false,
                    },
                ),
                PulseEvent::EmoteStop { address } => session.forward(
                    address,
                    PlayerMessage::Emote {
                        urn: String::new(),
                        incremental_id: 0,
                        stopping: true,
                    },
                ),
                // A peer entered our interest set. Report the arrival, then their initial profile
                // version; the version alone would register presence, but saying so explicitly
                // matches the other transports and doesn't depend on it carrying one.
                PulseEvent::Joined {
                    address,
                    profile_version,
                    parcel,
                    realm,
                    ..
                } => {
                    session.forward_at(address, &realm, parcel, PlayerMessage::Joined);
                    bridge_profile_version(session, address, profile_version);
                }
                // A later announcement. Bridged as an rfc4 `AnnounceProfileVersion` so it reuses the
                // same profile path as the byte transports; the set is idempotent.
                PulseEvent::ProfileVersion { address, version } => {
                    bridge_profile_version(session, address, version)
                }
                // The peer left our interest set (or disconnected). Report the departure on the
                // transport that was carrying it: presence is the union of the transports a peer is
                // seen on, so this drops them from the set and — once no transport is left —
                // despawns them. On a listening server that is the Pulse transport of whichever
                // scene context it was last placed in.
                PulseEvent::Left { address } => session.forget(address),
            }
        }
    }
}

/// Bridge a Pulse-announced profile version into the shared foreign-player pipeline as an rfc4
/// `AnnounceProfileVersion`, reusing the same handling as the LiveKit/websocket profile-version path
/// (`global_crdt` → `ProfileEvent::Version`). The address resolves to (or creates) the foreign player.
fn bridge_profile_version(session: &PulseSession, address: Address, version: i32) {
    session.forward(
        address,
        PlayerMessage::PlayerData(rfc4::packet::Message::ProfileVersion(
            rfc4::AnnounceProfileVersion {
                profile_version: version.max(0) as u32,
            },
        )),
    );
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
#[allow(clippy::too_many_arguments)]
fn on_handshake_response(
    session: &mut PulseSession,
    realm: &CurrentRealm,
    player: &Query<&GlobalTransform, With<PrimaryUser>>,
    in_world: bool,
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
    // A listener has no position to announce and is refused every message but `Resync`; its AoI went
    // out with the handshake.
    if session.listener.is_some() {
        return;
    }
    // Suppress the connect-time teleport while out of world — our position is provisional behind the
    // loading screen. The teleport sent when the player is placed in-world (`PlayerTeleported`, via
    // `pulse_teleport_on_local_move`) announces the real position + realm once we have one.
    if in_world {
        send_teleport(session, realm, player);
    } else {
        debug!("pulse: out of world at handshake, deferring teleport to spawn");
    }
}

/// Whether the current realm serves scenes off a local `dcl start` dev server, which the preview
/// server signals by listing the project's parcels in its `about`. Those realms all advertise the
/// same realm name, so their Pulse partition needs qualifying — see `resolve_realm_qualifier`.
fn is_local_realm(realm: &CurrentRealm) -> bool {
    realm
        .config
        .local_scene_parcels
        .as_ref()
        .is_some_and(|parcels| !parcels.is_empty())
}

/// Keep a listening server's routing in step with the scenes it hosts: which context owns each
/// hosted parcel, a Pulse `Transport` per context, and the AoI the whole lot adds up to.
/// Client-side this never runs (no listener, no server contexts).
fn update_listener_aoi(
    mut commands: Commands,
    session: Option<ResMut<PulseSession>>,
    definitions: Res<Assets<EntityDefinition>>,
    crdt_contexts: Res<CrdtContexts>,
    scene_realms: Res<SceneRealms>,
    realm: Res<CurrentRealm>,
    states: Query<&GlobalCrdtState>,
) {
    let Some(session) = session else {
        return;
    };
    let session = session.into_inner();
    if session.listener.is_none() {
        return;
    }
    // The realm for a scene that didn't come with one of its own. On a local preview this is the
    // machine-qualified name, not the bare `LocalPreview` every dev server advertises — a listener
    // has to land in the same partition its clients announce.
    let Some(default_realm) = announced_realm(session, &realm) else {
        return;
    };
    let Some(listener) = session.listener.as_mut() else {
        return;
    };

    // Every loaded entity definition with parcel pointers — which on a server is exactly the scenes
    // it hosts — mapped to the crdt context that scene runs on (one per room on an orchestrated
    // server, the shared one on the standalone local-dev server). Non-scene entities (profiles,
    // wearables) point at addresses and urns, so they fall out of the parse; a scene whose context
    // isn't registered yet simply isn't routable yet, and gets picked up on a later frame.
    let mut context_by_parcel: HashMap<String, HashMap<IVec2, Entity>> = HashMap::default();
    for (_, definition) in definitions.iter() {
        let mut parcels = definition.pointers.iter().filter_map(|pointer| {
            let (x, z) = pointer.split_once(',')?;
            Some(IVec2::new(x.trim().parse().ok()?, z.trim().parse().ok()?))
        });
        let Some(first) = parcels.next() else {
            continue;
        };
        let Some(context) = crdt_contexts.try_for_scene_hash(&definition.id) else {
            continue;
        };
        let realm_parcels = context_by_parcel
            .entry(scene_realms.for_scene_hash(&definition.id, &default_realm))
            .or_default();
        for parcel in std::iter::once(first).chain(parcels) {
            realm_parcels.insert(parcel, context);
        }
    }

    if listener.context_by_parcel == context_by_parcel {
        return;
    }

    // A context that no longer hosts anything loses its transport, which drops every peer it was
    // carrying (the despawn observer sweeps them out of `ForeignPlayer.transports`) — the same
    // teardown a scene room gets when its room goes away.
    listener.contexts.retain(|context, slot| {
        let keep = context_by_parcel
            .values()
            .any(|parcels| parcels.values().any(|c| c == context));
        if !keep {
            if let Ok(mut transport) = commands.get_entity(slot.transport) {
                transport.despawn();
            }
        }
        keep
    });

    for context in context_by_parcel
        .values()
        .flat_map(|parcels| parcels.values().copied())
    {
        if listener.contexts.contains_key(&context) {
            continue;
        }
        let Ok(state) = states.get(context) else {
            continue;
        };
        // A listener is never a subject, so nothing is ever sent on these: no `PulseOutbox` to drain
        // them, and the receiver is dropped here so anything a broadcast does queue is refused
        // rather than accumulating. They exist to own the context's Pulse presence and to be the
        // `transport_id` its inbound state is attributed to.
        let (sender, _) = mpsc::channel(1);
        let transport = commands
            .spawn(Transport {
                transport_type: TransportType::Pulse,
                sender,
                control: None,
                context,
            })
            .id();
        listener.contexts.insert(
            context,
            ListenerContext {
                transport,
                sender: state.get_sender(),
            },
        );
    }

    // Peers placed in a context that just went away are already gone with its transport.
    let live = &listener.contexts;
    listener
        .peer_context
        .retain(|_, context| live.contains_key(context));

    listener.context_by_parcel = context_by_parcel;

    // One entry per realm, its parcels consolidated into one rect per contiguous rectangular block
    // — blocks from adjacent scenes in the same realm merging is fine, the AoI is only a filter.
    // Sorted so an unchanged AoI compares equal across frames (HashMap iteration order does not).
    let mut aoi: Vec<_> = listener
        .context_by_parcel
        .iter()
        .map(|(realm, parcels)| pulse::SceneListenerAoi {
            realm: realm.clone(),
            parcel_rects: scene_regions(parcels.keys().copied())
                .into_iter()
                .map(|region| pulse::ParcelRect {
                    min_x: region.min.x,
                    min_z: region.min.y,
                    max_x: region.max.x,
                    max_z: region.max.y,
                })
                .collect(),
        })
        .collect();
    aoi.sort_by(|a, b| a.realm.cmp(&b.realm));

    set_listener_aoi(session, aoi);
}

/// Announce a new AoI: store it, and on a live connection tell the server about it.
///
/// The set is the announcement — before the handshake it just decides what the handshake carries,
/// and whether there is anything to connect for at all.
fn set_listener_aoi(session: &mut PulseSession, aoi: Vec<pulse::SceneListenerAoi>) {
    let Some(listener) = session.listener.as_mut() else {
        return;
    };
    if listener.aoi == aoi {
        return;
    }

    listener.aoi = aoi;
    // Nothing hosted yet → nothing to announce; `drive_connection` stays idle until there is.
    session.wanted = !listener.aoi.is_empty();
    // Before the handshake there is nothing to update: the set we just stored is what it will
    // announce. Afterwards the connection stays up and the server swaps the AoI in place.
    //
    // An empty set is not announceable — the server rejects a `SceneListenerUpdate` with no rects
    // and disconnects. On a live server it is also nearly always momentary: a scene reload drops
    // the old entity definition a frame before the new one lands. So hold the last announced AoI
    // and wait for the next non-empty one rather than cycling the connection over a reload.
    if session.wanted && matches!(session.state, Connection::Established) {
        send_listener_aoi(session);
    }
}

/// Announce the listener's current AoI on a live connection as a `SceneListenerUpdate`. The server
/// replaces the announced parcel set wholesale, keeping the connection, identity and realm — so
/// unlike a reconnect this costs no re-authentication and no gap in the positional stream.
fn send_listener_aoi(session: &PulseSession) {
    let (Some(link), Some(listener)) = (session.link.as_ref(), session.listener.as_ref()) else {
        return;
    };

    let message = pulse::ClientMessage {
        message: Some(pulse::client_message::Message::SceneListenerUpdate(
            pulse::SceneListenerUpdate {
                aoi: listener.aoi.clone(),
            },
        )),
    };
    let _ = link.outbound.try_send(PulseFrame {
        bytes: message.encode_to_vec(),
        reliability: PulseReliability::Reliable,
    });
    info!(
        "pulse: scene-listener AoI update sent ({})",
        describe_aoi(&listener.aoi)
    );
}

/// The realm string we announce to Pulse — the partition key every participant must derive
/// identically (the server compares realms with an ordinal string match). Empty/missing is rejected
/// by the server, and on a locally served realm it needs qualifying: every `dcl start` preview
/// advertises the same `LocalPreview` name, so the dev server's machine id separates two devs and
/// its port separates two servers on one machine. Both are properties of the *server*, so a LAN
/// preview peer derives the same string — a locally computed machine id would not. `None` until the
/// realm name is known, and on a local realm until a `b64-` addressed scene has loaded.
fn announced_realm(session: &PulseSession, realm: &CurrentRealm) -> Option<String> {
    let realm_name = realm
        .config
        .realm_name
        .clone()
        .filter(|name| !name.is_empty())
        .or_else(|| {
            warn!("pulse: no realm name yet (no peers will be visible)");
            None
        })?;

    if !is_local_realm(realm) {
        return Some(realm_name);
    }

    let Some(qualifier) = session.realm_qualifier.as_ref() else {
        debug!("pulse: local realm without a machine id yet; deferring announce");
        return None;
    };
    Some(
        match realm
            .address
            .parse::<http::Uri>()
            .ok()
            .and_then(|url| url.port_u16())
        {
            Some(port) => format!("{realm_name}-{qualifier}-{port}"),
            None => format!("{realm_name}-{qualifier}"),
        },
    )
}

/// Recover the dev server's machine id from a loaded scene's `b64-` content hashes and cache it on
/// the session, then re-announce if we already handshook without it. Every client served by one dev
/// server sees the same hashes, so all of them derive the same qualifier — which is what a machine
/// id computed locally could not do (a LAN/QR preview peer would compute its own).
fn resolve_realm_qualifier(
    session: Option<ResMut<PulseSession>>,
    realm: Res<CurrentRealm>,
    definitions: Res<Assets<EntityDefinition>>,
    player: Query<&GlobalTransform, With<PrimaryUser>>,
    out_of_world: Query<(), (With<PrimaryUser>, With<OutOfWorld>)>,
) {
    let Some(session) = session else {
        return;
    };
    let session = session.into_inner();
    if realm.is_changed() {
        session.realm_qualifier = None;
    }
    if session.realm_qualifier.is_some() || !is_local_realm(&realm) {
        return;
    }

    let Some(qualifier) = definitions.iter().find_map(|(_, definition)| {
        definition
            .content
            .values()
            .find_map(|(file, hash)| machine_id_from_b64(file, hash))
    }) else {
        return;
    };

    info!("pulse: local realm qualified by machine id {qualifier}");
    session.realm_qualifier = Some(qualifier);
    if matches!(session.state, Connection::Established) && out_of_world.is_empty() {
        send_teleport(session, &realm, &player);
    }
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
    let world = player
        .single()
        .map(|t| t.translation())
        .unwrap_or(Vec3::ZERO);
    send_teleport_at(session, realm, world);
}

/// Send a `TeleportRequest` for an explicit Bevy world-space position. Used by [`send_teleport`] (the
/// player's current position) and by [`pulse_teleport_on_local_move`] (an instant move, whose final
/// position is passed directly so it doesn't depend on transform propagation having run this frame).
fn send_teleport_at(session: &PulseSession, realm: &CurrentRealm, world: Vec3) {
    let Some(link) = session.link.as_ref() else {
        return;
    };

    let Some(realm_name) = announced_realm(session, realm) else {
        return;
    };

    // Bevy render position → DCL world coords (the `-z` flip), then split into parcel + local —
    // exactly the inverse of how inbound state is decoded.
    let dcl = DclTranslation::from_bevy_translation(world).0;
    let (parcel_index, local) = session
        .grid
        .encode_to_parcel(Vec3::new(dcl[0], dcl[1], dcl[2]));

    let teleport = pulse::TeleportRequest {
        parcel_index,
        position_x: pulse::TeleportRequest::position_x_quantized(local.x),
        position_y: pulse::TeleportRequest::position_y_quantized(local.y),
        position_z: pulse::TeleportRequest::position_z_quantized(local.z),
        realm: realm_name.clone(),
    };
    let message = pulse::ClientMessage {
        message: Some(pulse::client_message::Message::Teleport(teleport)),
    };
    let _ = link.outbound.try_send(PulseFrame {
        bytes: message.encode_to_vec(),
        reliability: PulseReliability::Reliable,
    });
    info!("pulse: teleport sent (realm {realm_name}, parcel {parcel_index})");
}

/// The local player was instantly repositioned (durationless `move_player_to`, `teleport_player`, a
/// spawn snap). Announce it to the Pulse server as a `TeleportRequest` so peers snap to the new
/// position instead of interpolating across the jump. The event carries the final world position, so
/// this doesn't depend on `GlobalTransform` propagation having run this frame.
fn pulse_teleport_on_local_move(
    session: Option<ResMut<PulseSession>>,
    realm: Res<CurrentRealm>,
    mut events: EventReader<PlayerTeleported>,
) {
    // Coalesce to the latest reposition this frame; earlier ones are superseded.
    let Some(position) = events.read().last().map(|ev| ev.position) else {
        return;
    };
    let Some(session) = session else {
        return;
    };
    // A listener is never a subject: it has no position to announce, and the server refuses every
    // post-auth message but `Resync`.
    if session.listener.is_none() && matches!(session.state, Connection::Established) {
        send_teleport_at(&session, &realm, position);
    }
}

/// Build a `SceneListenerHandshakeRequest`: the same signed-fetch auth chain as the player handshake
/// (the server authenticates both through one pipeline), plus the announced AoI — the parcels this
/// server hosts, per realm. No initial state — a listener is never a subject.
async fn build_listener_handshake_request(
    wallet: &Wallet,
    server_id: &str,
    aoi: Vec<pulse::SceneListenerAoi>,
) -> Result<pulse::SceneListenerHandshakeRequest, String> {
    Ok(pulse::SceneListenerHandshakeRequest {
        auth_chain: build_auth_chain(wallet, server_id).await?,
        aoi,
    })
}

/// `2 realms, 5 rects` — the shape of an announcement, for logs.
fn describe_aoi(aoi: &[pulse::SceneListenerAoi]) -> String {
    let rects: usize = aoi.iter().map(|realm| realm.parcel_rects.len()).sum();
    format!("{} realms, {rects} rects", aoi.len())
}

/// Build a `HandshakeRequest`: sign `connect:/{server_id}:{ts}:{}` with the local identity and pack
/// the resulting auth chain into the platform's canonical `x-identity-*` dictionary (JSON object,
/// every value a string), serialized as UTF-8 bytes — identical in shape to the HTTP signed-fetch
/// headers, just carried in a protobuf `bytes` field. Mirrors Unity's `BuildAuthChain`.
async fn build_handshake_request(
    wallet: &Wallet,
    server_id: &str,
    profile_version: i32,
) -> Result<pulse::HandshakeRequest, String> {
    Ok(pulse::HandshakeRequest {
        auth_chain: build_auth_chain(wallet, server_id).await?,
        profile_version,
        initial_state: None,
    })
}

/// The signed-fetch auth chain both handshakes carry: sign `connect:/{server_id}:{ts}:{}` with the
/// local identity and pack the resulting chain into the platform's canonical `x-identity-*`
/// dictionary (JSON object, every value a string) as UTF-8 bytes — identical in shape to the HTTP
/// signed-fetch headers, just carried in a protobuf `bytes` field. Mirrors Unity's `BuildAuthChain`.
async fn build_auth_chain(wallet: &Wallet, server_id: &str) -> Result<Vec<u8>, String> {
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
    serde_json::to_vec(&dict).map_err(|e| e.to_string())
}
