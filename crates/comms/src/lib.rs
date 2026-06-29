pub mod archipelago;
pub mod broadcast_position;
pub mod global_crdt;
#[cfg(feature = "livekit")]
pub mod livekit;
pub mod movement_compressed;
pub mod preview;
pub mod profile;
pub mod pulse;
pub mod signed_login;
#[cfg(test)]
mod test;
#[cfg(feature = "transport_debug")]
mod transport_debug;
pub mod websocket_room;

use std::marker::PhantomData;

use bevy::{
    ecs::system::SystemParam,
    prelude::*,
    tasks::{IoTaskPool, Task},
};
use bimap::BiMap;
use common::{
    structs::{CurrentRealm, MicState},
    util::{TaskCompat, TaskExt},
};
use ethers_core::types::{Address, H160};
use http::{StatusCode, Uri};
use preview::PreviewPlugin;
use serde::{Deserialize, Serialize};
use signed_login::{SignedLoginPlugin, StartSignedLogin};
use tokio::sync::mpsc::Sender;

use dcl_component::{
    proto_components::{kernel::comms::rfc4, pulse as pulse_proto},
    DclWriter, ToDclWriter,
};
use ipfs::IpfsAssetServer;
use prost::Message as _;
use wallet::{sign_request, Wallet};

use crate::global_crdt::ChannelControl;

use self::{
    archipelago::{ArchipelagoPlugin, StartArchipelago},
    broadcast_position::BroadcastPositionPlugin,
    global_crdt::GlobalCrdtPlugin,
    profile::UserProfilePlugin,
    pulse::{
        from_movement,
        transport::{PulseFrame, PulseReliability},
        PulseCtx,
    },
    websocket_room::{StartWsRoom, WebsocketRoomPlugin},
};

#[cfg(feature = "livekit")]
use self::livekit::{plugin::LivekitPlugin, StartLivekit};

const GATEKEEPER_URL: &str = "https://comms-gatekeeper.decentraland.org/get-scene-adapter";
const PREVIEW_GATEKEEPER_URL: &str =
    "https://comms-gatekeeper-local.decentraland.org/get-scene-adapter";

pub mod chat_marker_things {
    pub const EMOTE: char = '␐';

    pub const ALL: [char; 3] = [EMOTE, '␑', '␆'];
}

pub struct CommsPlugin;

impl Plugin for CommsPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<SetCurrentScene>()
            .init_resource::<SceneRoomConnection>();

        app.add_plugins((
            WebsocketRoomPlugin,
            SignedLoginPlugin,
            ArchipelagoPlugin,
            BroadcastPositionPlugin,
            GlobalCrdtPlugin,
            UserProfilePlugin,
            PreviewPlugin,
        ));

        #[cfg(feature = "livekit")]
        app.add_plugins(LivekitPlugin);
        app.init_resource::<MicState>();

        // Pulse movement transport. Inert until a `pulse::plugin::PulseConfig` resource is
        // inserted; the driver (native ENet thread / wasm no-op) is selected at compile time.
        app.add_plugins(pulse::plugin::PulsePlugin);

        app.add_systems(Update, (process_realm_change, connect_scene_room));

        #[cfg(feature = "transport_debug")]
        app.add_plugins(transport_debug::TransportDebugPlugin);
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum TransportType {
    WebsocketRoom,
    Livekit,
    Archipelago,
    /// The realm's high-frequency avatar-state carrier (UDP/ENet). A real transport entity whose
    /// channel feeds a bridge that converts rfc4 bytes into Pulse `ClientMessage`s; spawned only on
    /// livekit realms (see `pulse::plugin`).
    Pulse,
}

bitflags::bitflags! {
    /// Which transports a broadcast targets, so a sender declares intent once (`broadcast_to`)
    /// instead of repeating `transport_type != X` filters at every call site. Flags compose, so a
    /// message bound for several transports is a single `broadcast_to` call (e.g.
    /// `PRIMARY | ARCHIPELAGO`) rather than one per transport. Pulse is just another transport here:
    /// its entity's channel feeds a bridge that converts the rfc4 bytes into Pulse `ClientMessage`s
    /// (see `pulse::plugin`), and drops anything it can't convert — so only `PULSE`-targeted
    /// (avatar-state) messages should carry the bit.
    #[derive(Clone, Copy, Debug)]
    pub struct BroadcastTarget: u8 {
        /// The local websocket dev server (the realm's non-LiveKit avatar-state byte transport).
        const WEBSOCKET = 1 << 0;
        const LIVEKIT = 1 << 1;
        /// The Archipelago island-assignment transport.
        const ARCHIPELAGO = 1 << 2;
        /// The realm's Pulse avatar-state transport (only carries convertible avatar state).
        const PULSE = 1 << 4;

        /// Realm avatar state: the websocket dev server plus Pulse. Movement rides this.
        const PRIMARY = Self::WEBSOCKET.bits() | Self::PULSE.bits();
    }
}

impl BroadcastTarget {
    fn flag_for(transport_type: &TransportType) -> BroadcastTarget {
        match transport_type {
            TransportType::WebsocketRoom => BroadcastTarget::WEBSOCKET,
            TransportType::Livekit => BroadcastTarget::LIVEKIT,
            TransportType::Archipelago => BroadcastTarget::ARCHIPELAGO,
            TransportType::Pulse => BroadcastTarget::PULSE,
        }
    }

    pub fn includes(self, transport_type: &TransportType) -> bool {
        self.contains(Self::flag_for(transport_type))
    }
}

#[derive(Clone, Copy, Debug)]
pub enum NetworkMessageRecipient {
    All,
    Peer(H160),
    AuthServer,
}

/// A queued broadcast that carries its own per-transport encodings rather than pre-resolved bytes,
/// so a single message reaches transports that want different wire forms. Each receive side pulls the
/// representation it needs off [`Broadcast`]: byte transports (websocket/livekit/archipelago) take
/// `to_rfc4`; the Pulse transport takes `to_pulse`. Byte-only sends box already-encoded rfc4 bytes
/// (a `Vec<u8>`), while dual-representation messages (movement, emote) build each form on demand.
pub struct NetworkMessage {
    pub(crate) message: Box<dyn Broadcast>,
    pub unreliable: bool,
    pub recipient: NetworkMessageRecipient,
}

impl NetworkMessage {
    pub fn unreliable<D: ToDclWriter>(message: &D) -> Self {
        let mut data = Vec::new();
        let mut writer = DclWriter::new(&mut data);
        message.to_writer(&mut writer);
        Self {
            message: Box::new(data),
            unreliable: true,
            recipient: NetworkMessageRecipient::All,
        }
    }

    pub fn reliable<D: ToDclWriter>(message: &D) -> Self {
        Self {
            unreliable: false,
            ..Self::unreliable(message)
        }
    }

    pub fn targetted_reliable<D: ToDclWriter>(
        message: &D,
        recipient: NetworkMessageRecipient,
    ) -> Self {
        Self {
            unreliable: false,
            recipient,
            ..Self::unreliable(message)
        }
    }
}

/// A message's per-transport representations. A type implements only the forms it has: the trivial
/// `Vec<u8>` carrier is already-encoded rfc4 bytes (no Pulse form), while dual-representation types
/// (movement, emote) build both, so each transport gets its native encoding without the other having
/// to re-decode. The receive side of every transport pulls the form it wants through this trait.
pub trait Broadcast: Send + Sync {
    /// rfc4 wire bytes for byte transports. `None` if this message has no rfc4 form.
    fn to_rfc4(&self) -> Option<Vec<u8>>;
    /// A Pulse frame for the Pulse transport. `None` (the default) if there's no Pulse form — so a
    /// byte-only message queued onto the Pulse transport is simply dropped there.
    fn to_pulse(&self, ctx: &mut PulseCtx) -> Option<PulseFrame> {
        let _ = ctx;
        None
    }
}

/// The trivial carrier: already-encoded rfc4 bytes. Byte-only senders encode upfront (via the
/// [`NetworkMessage`] constructors / [`broadcast_to`]) and box the bytes; Pulse drops them.
impl Broadcast for Vec<u8> {
    fn to_rfc4(&self) -> Option<Vec<u8>> {
        Some(self.clone())
    }
}

impl Broadcast for rfc4::Movement {
    fn to_rfc4(&self) -> Option<Vec<u8>> {
        Some(encode_packet(rfc4::packet::Message::Movement(self.clone())))
    }

    fn to_pulse(&self, ctx: &mut PulseCtx) -> Option<PulseFrame> {
        let state = from_movement(self, ctx.grid);
        // Cache the full state so a following emote can attach it — the Pulse server rejects an
        // `EmoteStart` with a null `player_state`.
        *ctx.last_state = Some(state.clone());
        Some(PulseFrame {
            bytes: pulse_proto::ClientMessage {
                message: Some(pulse_proto::client_message::Message::Input(
                    pulse_proto::PlayerStateInput { state: Some(state) },
                )),
            }
            .encode_to_vec(),
            reliability: PulseReliability::UnreliableSequenced,
        })
    }
}

/// A local emote start or stop. Dual-representation: an rfc4 `PlayerEmote` for byte transports and a
/// Pulse `EmoteStart`/`EmoteStop` for Pulse. Sent via [`broadcast`].
#[derive(Clone)]
pub struct Emote {
    pub urn: String,
    pub incremental_id: u32,
    pub timestamp: f32,
    /// `Some` for a one-shot (the Pulse server auto-completes it after the duration); `None` for a
    /// looping emote (ended by a later `stopping` send). Ignored on a stop.
    pub duration_ms: Option<u32>,
    /// A stop: clears a looping emote. rfc4 `is_stopping = true` / Pulse `EmoteStop`.
    pub stopping: bool,
}

impl Broadcast for Emote {
    fn to_rfc4(&self) -> Option<Vec<u8>> {
        Some(encode_packet(rfc4::packet::Message::PlayerEmote(
            rfc4::PlayerEmote {
                incremental_id: self.incremental_id,
                urn: self.urn.clone(),
                timestamp: self.timestamp,
                is_stopping: Some(self.stopping),
            },
        )))
    }

    fn to_pulse(&self, ctx: &mut PulseCtx) -> Option<PulseFrame> {
        let message = if self.stopping {
            pulse_proto::client_message::Message::EmoteStop(pulse_proto::EmoteStop {})
        } else {
            // The server rejects an `EmoteStart` with a null `player_state`. If we haven't sent any
            // movement yet there's nothing to attach, so skip the start rather than be disconnected;
            // an emote after the first movement goes out fine.
            let state = ctx.last_state.clone()?;
            pulse_proto::client_message::Message::EmoteStart(pulse_proto::EmoteStart {
                emote_id: self.urn.clone(),
                duration_ms: self.duration_ms,
                player_state: Some(state),
            })
        };
        Some(PulseFrame {
            bytes: pulse_proto::ClientMessage {
                message: Some(message),
            }
            .encode_to_vec(),
            reliability: PulseReliability::Reliable,
        })
    }
}

impl Broadcast for rfc4::AnnounceProfileVersion {
    fn to_rfc4(&self) -> Option<Vec<u8>> {
        Some(encode_packet(rfc4::packet::Message::ProfileVersion(
            self.clone(),
        )))
    }

    fn to_pulse(&self, _ctx: &mut PulseCtx) -> Option<PulseFrame> {
        Some(PulseFrame {
            bytes: pulse_proto::ClientMessage {
                message: Some(pulse_proto::client_message::Message::ProfileAnnouncement(
                    pulse_proto::ProfileVersionAnnouncement {
                        version: self.profile_version as i32,
                    },
                )),
            }
            .encode_to_vec(),
            reliability: PulseReliability::Reliable,
        })
    }
}

/// A local profile update. Dual-representation: byte transports get the full rfc4 `ProfileResponse`
/// (peers apply it directly), while Pulse — which carries no profile payload — gets only a
/// `ProfileVersionAnnouncement`, on which peers refetch the profile from catalyst.
#[derive(Clone)]
pub struct ProfileUpdate {
    pub serialized_profile: String,
    pub base_url: String,
    pub version: u32,
}

impl Broadcast for ProfileUpdate {
    fn to_rfc4(&self) -> Option<Vec<u8>> {
        Some(encode_packet(rfc4::packet::Message::ProfileResponse(
            rfc4::ProfileResponse {
                serialized_profile: self.serialized_profile.clone(),
                base_url: self.base_url.clone(),
            },
        )))
    }

    fn to_pulse(&self, _ctx: &mut PulseCtx) -> Option<PulseFrame> {
        Some(PulseFrame {
            bytes: pulse_proto::ClientMessage {
                message: Some(pulse_proto::client_message::Message::ProfileAnnouncement(
                    pulse_proto::ProfileVersionAnnouncement {
                        version: self.version as i32,
                    },
                )),
            }
            .encode_to_vec(),
            reliability: PulseReliability::Reliable,
        })
    }
}

/// Encode an rfc4 oneof body into a protocol-version-100 `Packet`'s wire bytes.
fn encode_packet(message: rfc4::packet::Message) -> Vec<u8> {
    let mut data = Vec::new();
    let mut writer = DclWriter::new(&mut data);
    rfc4::Packet {
        message: Some(message),
        protocol_version: 100,
    }
    .to_writer(&mut writer);
    data
}

#[derive(Component)]
pub struct Transport {
    pub transport_type: TransportType,
    pub sender: Sender<NetworkMessage>,
    pub control: Option<Sender<ChannelControl>>,
    pub foreign_aliases: BiMap<u32, Address>,
}

/// Encode `message` once and queue it onto every transport matching `target`. The single point that
/// turns a [`BroadcastTarget`] into per-transport sends; full channels are dropped (best-effort).
pub fn broadcast_to<'a, D: ToDclWriter>(
    transports: impl Iterator<Item = &'a Transport>,
    target: BroadcastTarget,
    unreliable: bool,
    message: &D,
) {
    let mut data = Vec::new();
    let mut writer = DclWriter::new(&mut data);
    message.to_writer(&mut writer);
    for transport in transports.filter(|t| target.includes(&t.transport_type)) {
        let _ = transport.sender.try_send(NetworkMessage {
            message: Box::new(data.clone()),
            unreliable,
            recipient: NetworkMessageRecipient::All,
        });
    }
}

/// Queue a [`Broadcast`] message onto every transport matching `target`, boxed fresh per transport so
/// each receive side pulls its own representation ([`Broadcast::to_rfc4`] for byte transports,
/// [`Broadcast::to_pulse`] for Pulse). The counterpart to [`broadcast_to`] for messages that carry a
/// native (non-rfc4-bytes) form — currently [`rfc4::Movement`] and [`Emote`], both sent on
/// [`BroadcastTarget::PRIMARY`] (movement unreliable, emote reliable).
pub fn broadcast<'a, B: Broadcast + Clone + 'static>(
    transports: impl Iterator<Item = &'a Transport>,
    target: BroadcastTarget,
    unreliable: bool,
    message: B,
) {
    for transport in transports.filter(|t| target.includes(&t.transport_type)) {
        let _ = transport.sender.try_send(NetworkMessage {
            message: Box::new(message.clone()),
            unreliable,
            recipient: NetworkMessageRecipient::All,
        });
    }
}

fn process_realm_change(
    mut commands: Commands,
    realm: Res<CurrentRealm>,
    adapters: Query<Entity, With<Transport>>,
    mut manager: AdapterManager,
    wallet: Res<Wallet>,
) {
    if realm.is_changed() || wallet.is_changed() {
        for adapter in adapters.iter() {
            commands.entity(adapter).despawn();
        }

        if wallet.address().is_none() {
            info!("disconnecting comms, no identity");
            return;
        }

        if let Some(comms) = realm.comms.as_ref() {
            if let Some(adapter) = comms.adapter.as_ref() {
                let real_adapter = adapter
                    .split_once(':')
                    .map(|(_, tail)| tail)
                    .unwrap_or(adapter.as_str());
                manager.connect(real_adapter);
            } else if let Some(adapter) = comms.fixed_adapter.as_ref() {
                manager.connect(adapter);
            }
        } else {
            debug!("missing comms!");
        }
    }
}

#[derive(Serialize, Event, Clone, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SetCurrentScene {
    pub realm_name: String,
    pub scene_id: String,
}

#[derive(Serialize, Deserialize)]
pub struct GatekeeperResponse {
    adapter: String,
}

#[derive(Component)]
pub struct SceneRoom(pub String);

#[derive(Resource, Default)]
pub struct SceneRoomConnection(pub Option<(SetCurrentScene, String, Entity)>);

#[allow(clippy::type_complexity)]
fn connect_scene_room(
    mut commands: Commands,
    mut manager: AdapterManager,
    mut gatekeeper_task: Local<Option<Task<Result<(String, SetCurrentScene), anyhow::Error>>>>,
    mut current: ResMut<SceneRoomConnection>,
    mut scene: EventReader<SetCurrentScene>,
    wallet: Res<Wallet>,
    ipfs: IpfsAssetServer,
) {
    if let Some(ev) = scene.read().last().cloned() {
        if let Some((existing, room, entity)) = current.0.take() {
            if existing == ev {
                current.0 = Some((existing, room, entity));
                return;
            }
            if let Ok(mut commands) = commands.get_entity(entity) {
                commands.despawn();
            }
            warn!("disconnected scene channel {ev:?}");
        }
        if ev.scene_id.is_empty() {
            *gatekeeper_task = None;
        } else {
            let wallet = wallet.clone();
            let url = if ev.scene_id.starts_with("b64-") {
                PREVIEW_GATEKEEPER_URL
            } else {
                GATEKEEPER_URL
            };
            let uri = Uri::try_from(url).unwrap();
            let client = ipfs.ipfs().client();
            *gatekeeper_task = Some(IoTaskPool::get().spawn_compat(async move {
                let headers =
                    sign_request("POST", &uri, &wallet, serde_json::to_string(&ev).unwrap())
                        .await?;

                let mut request = client
                    .post(uri.to_string())
                    .timeout(std::time::Duration::from_secs(10))
                    .header("Content-Type", "application/json");
                for (k, v) in headers {
                    request = request.header(k, v);
                }
                let response = request.send().await?;

                if response.status() != StatusCode::OK {
                    return Err(anyhow::anyhow!("status: {}", response.status()));
                }

                Ok((response.json::<GatekeeperResponse>().await?.adapter, ev))
            }));
        }
    }

    if let Some(mut task) = gatekeeper_task.take() {
        match task.complete() {
            None => *gatekeeper_task = Some(task),
            Some(Err(e)) => warn!("failed to get scene room from gatekeeper: {e}"),
            Some(Ok((adapter, ev))) => {
                if let Some(ent) = manager.connect_scene(&adapter) {
                    warn!("added scene channel {ev:?}");
                    commands
                        .entity(ent)
                        .try_insert(SceneRoom(ev.scene_id.clone()));
                    current.0 = Some((ev, adapter, ent));
                }
            }
        }
    }
}

#[derive(SystemParam)]
pub struct AdapterManager<'w, 's> {
    #[cfg(feature = "livekit")]
    commands: Commands<'w, 's>,
    ws_room_events: EventWriter<'w, StartWsRoom>,
    #[cfg(feature = "livekit")]
    livekit_events: EventWriter<'w, StartLivekit>,
    // Pulse rides alongside livekit realms; spawned from the livekit arm of `connect`.
    #[cfg(feature = "livekit")]
    pulse_events: EventWriter<'w, pulse::plugin::StartPulse>,
    archipelago_events: EventWriter<'w, StartArchipelago>,
    // can't use event writer due to conflict on Res<Events>
    pub signed_login_events: ResMut<'w, Events<StartSignedLogin>>,
    _p: PhantomData<&'s ()>,
}

impl AdapterManager<'_, '_> {
    /// Connect the realm's island comms. A livekit island also brings up the realm's Pulse
    /// avatar-state transport.
    pub fn connect(&mut self, adapter: &str) -> Option<Entity> {
        self.connect_inner(adapter, true)
    }

    /// Connect a per-scene messagebus room. Even when it resolves to livekit it must NOT bring up
    /// Pulse: Pulse is the *realm's* avatar-state transport, not a per-scene room. A scene room is
    /// distinguished only by its [`SceneRoom`] marker — its `TransportType` is its wire protocol
    /// (livekit/ws-room), so the realm island and a livekit scene room are otherwise identical here.
    pub fn connect_scene(&mut self, adapter: &str) -> Option<Entity> {
        self.connect_inner(adapter, false)
    }

    #[cfg_attr(not(feature = "livekit"), allow(unused_variables))]
    fn connect_inner(&mut self, adapter: &str, is_realm_island: bool) -> Option<Entity> {
        let Some((protocol, address)) = adapter.split_once(':') else {
            warn!("unrecognised adapter string: {adapter}");
            return None;
        };

        match protocol {
            "ws-room" => {
                self.ws_room_events.write(StartWsRoom {
                    address: address.to_owned(),
                });
            }
            "signed-login" => {
                self.signed_login_events.send(StartSignedLogin {
                    address: address.to_owned(),
                });
            }
            #[cfg(feature = "livekit")]
            "livekit" => {
                let entity = self.commands.spawn_empty().id();
                self.livekit_events.write(StartLivekit {
                    entity,
                    address: address.to_owned(),
                });
                // A livekit *realm island* is a Pulse realm: (re)spawn the Pulse routing transport
                // and announce the new realm. On the first such realm this also establishes the
                // connection; on later ones it just re-teleports. A livekit *scene room* lands here
                // too but must not touch Pulse, hence the realm-island gate.
                if is_realm_island {
                    self.pulse_events.write(pulse::plugin::StartPulse);
                }
                return Some(entity);
            }
            #[cfg(not(feature = "livekit"))]
            "livekit" => {
                info!("livekit not enabled: comms offline");
            }
            "offline" => {
                info!("comms offline");
            }
            "archipelago" => {
                debug!("arch starting: {address}");
                self.archipelago_events.write(StartArchipelago {
                    address: address.to_owned(),
                });
            }
            "fixed-adapter" => {
                // fixed-adapter should be ignored and we use the tail as the full protocol:address
                return self.connect_inner(address, is_realm_island);
            }
            _ => {
                warn!("unrecognised adapter protocol: {protocol}");
            }
        }

        None
    }
}
