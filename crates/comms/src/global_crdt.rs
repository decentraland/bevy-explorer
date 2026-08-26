use std::{f32::consts::TAU, sync::Arc};

use bevy::{
    app::Propagate,
    platform::collections::{HashMap, HashSet},
    prelude::*,
    render::view::RenderLayers,
};
use bimap::BiMap;
use common::{
    rpc::{RpcCall, RpcEventSender, RpcStreamSender},
    structs::{
        AudioDecoderError, EmoteCommand, GlobalCrdtStateUpdate, HeadSync, MoveKind, PointAtSync,
        SceneDrivenAnimationRequest,
    },
    util::ModifyComponentExt,
};
use ethers_core::types::Address;
use serde::{Deserialize, Serialize};
use serde_json::json;
use system_bridge::{SystemApi, VoiceMessage};
use tokio::sync::{broadcast, mpsc, oneshot};

use dcl::{
    crdt::{append_component, delete_entity, put_component},
    interface::{CrdtStore, CrdtType},
    js::comms::CommsMessageType,
};
use dcl_component::{
    proto_components::{
        kernel::comms::rfc4::{self, packet::Message},
        sdk::components::PbPlayerIdentityData,
    },
    transform_and_parent::{DclQuat, DclTransformAndParent, DclTranslation},
    DclReader, DclWriter, GlobalCrdtData, Localizer, SceneComponentId, SceneEntityId, SceneOrigin,
};

use crate::{profile::ProfileMetaCache, Transport};

#[cfg(not(target_arch = "wasm32"))]
use kira::sound::streaming::StreamingSoundData;

#[cfg(target_arch = "wasm32")]
pub struct StreamingSoundData<T>(std::marker::PhantomData<fn() -> T>);

/// Allocates foreign-player entity ids within `SceneEntityId::FOREIGN_PLAYER_RANGE` for a
/// single crdt context. Freed ids are re-issued with a bumped generation so scenes treat
/// the recycled id as a fresh entity.
#[derive(Default)]
struct PlayerIdAllocator {
    free: Vec<SceneEntityId>,
    next_fresh: u16,
}

impl PlayerIdAllocator {
    fn alloc(&mut self) -> Option<SceneEntityId> {
        if let Some(id) = self.free.pop() {
            return Some(id);
        }
        let range = SceneEntityId::FOREIGN_PLAYER_RANGE;
        let id = range.start().checked_add(self.next_fresh)?;
        if id > *range.end() {
            return None;
        }
        self.next_fresh += 1;
        Some(SceneEntityId::new(id, 0))
    }

    fn free(&mut self, id: SceneEntityId) {
        self.free
            .push(SceneEntityId::new(id.id, id.generation.wrapping_add(1)));
    }
}

pub struct GlobalCrdtPlugin;

/// Drop incoming `NetworkUpdate::Player` messages without spawning
/// ForeignPlayer entities or updating the profile cache. Set by headless
/// consumers (e.g. the `impost` baker) that don't need remote-player state
/// and would otherwise pay for unbounded ProfileCache growth, foreign
/// entity churn, and the associated despawn-timeout work over long runs.
#[derive(Resource, Default)]
pub struct DiscardPlayerUpdates(pub bool);

impl Plugin for GlobalCrdtPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DiscardPlayerUpdates>();

        // the shared context: the client's single player view, which every transport and
        // scene resolves to unless a room-specific context exists for its hash. An
        // orchestrated server spawns one additional context per scene room.
        let shared = app.world_mut().spawn(GlobalCrdtState::new(None)).id();
        let mut contexts = CrdtContexts::default();
        contexts.0.insert(String::new(), shared);
        app.insert_resource(contexts);
        app.init_resource::<SceneRealms>();

        app.add_observer(remove_context_players);

        let (sender, _) = tokio::sync::broadcast::channel(1_000);
        app.insert_resource(LocalAudioSource { sender });

        app.init_resource::<VoiceMessageStreams>();

        app.add_systems(Update, process_transport_updates);
        app.add_systems(Update, despawn_players);
        app.add_observer(remove_transport_from_foreign_audio_source);
        app.add_observer(remove_transport_from_foreign_players);
        app.add_systems(Update, handle_foreign_audio);
        app.add_systems(
            Update,
            (
                drop_closed_voice_message_senders,
                receive_new_voice_message_senders.run_if(on_event::<SystemApi>),
            )
                .chain(),
        );
        app.add_event::<PlayerPositionEvent>();
        app.add_event::<PlayerSceneAnimEvent>();
        app.add_event::<ProfileEvent>();
        app.add_event::<ChatEvent>();
    }
}

// `PlayerData` wraps the full rfc4 message set, whose largest variants (Movement, the new
// SceneDrivenAnimation) dwarf the others. Boxing it would add a heap allocation on every inbound
// message — this is the hot path — so keep it inline and silence the size-difference lint.
#[allow(clippy::large_enum_variant)]
pub enum PlayerMessage {
    /// The transport saw this peer join, with nothing to report beyond that. Presence is registered
    /// by a `PlayerUpdate` arriving at all (see `NetworkUpdate::Player`), so this carries no payload
    /// — it exists so a transport can say "they are here" without waiting for them to send data.
    /// Transports that learn about joins should emit it; `PlayerLeft` is the counterpart.
    Joined,
    MetaData(String),
    PlayerData(rfc4::packet::Message),
    /// Pulse-decoded movement, delivered natively rather than as an rfc4 `Movement` packet (those
    /// are no longer supported). Boxed because the decoder already hands us a `Box`. `teleport` marks
    /// a discontinuous reposition (`TeleportPerformed`) so foreign dynamics snaps rather than lerps.
    Movement {
        movement: Box<rfc4::Movement>,
        teleport: bool,
        /// Server tick in seconds. `f64`, and carried beside the `rfc4::Movement` rather than in
        /// its proto `float` timestamp, which is too narrow for an absolute millisecond tick.
        timestamp: f64,
    },
    /// Pulse-decoded emote start (`stopping: false`) or stop, delivered natively for the same reason
    /// as [`PlayerMessage::Movement`]: an rfc4 `PlayerEmote` on a byte transport is a duplicate to be
    /// dropped, so the Pulse copy must be distinguishable from it by variant.
    Emote {
        urn: String,
        /// The server tick the emote started on; ordering only.
        incremental_id: u32,
        stopping: bool,
    },
    AudioStreamAvailable {
        transport: Entity,
    },
    AudioStreamUnavailable {
        transport: Entity,
    },
}

impl std::fmt::Debug for PlayerMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let var_name = match self {
            Self::Joined => f.write_str("Joined"),
            Self::MetaData(arg0) => f.debug_tuple("MetaData").field(arg0).finish(),
            Self::PlayerData(arg0) => f.debug_tuple("PlayerData").field(arg0).finish(),
            Self::Movement {
                movement,
                teleport,
                timestamp,
            } => f
                .debug_struct("Movement")
                .field("movement", movement)
                .field("teleport", teleport)
                .field("timestamp", timestamp)
                .finish(),
            Self::Emote {
                urn,
                incremental_id,
                stopping,
            } => f
                .debug_struct("Emote")
                .field("urn", urn)
                .field("incremental_id", incremental_id)
                .field("stopping", stopping)
                .finish(),
            Self::AudioStreamAvailable { transport } => f
                .debug_tuple("AudioStreamAvailable")
                .field(transport)
                .finish(),
            Self::AudioStreamUnavailable { transport } => f
                .debug_tuple("AudioStreamUnavailable")
                .field(transport)
                .finish(),
        };
        var_name
    }
}

#[derive(Debug)]
pub struct PlayerUpdate {
    pub transport_id: Entity,
    pub message: PlayerMessage,
    pub address: Address,
}

#[derive(Debug)]
pub struct NonPlayerUpdate {
    pub transport_id: Entity,
    pub address: String,
    pub message: rfc4::packet::Message,
}

#[derive(Debug)]
pub enum NetworkUpdate {
    Player(PlayerUpdate),
    NonPlayer(NonPlayerUpdate),
    PlayerLeft {
        transport_id: Entity,
        address: Address,
    },
}

impl From<PlayerUpdate> for NetworkUpdate {
    fn from(value: PlayerUpdate) -> Self {
        NetworkUpdate::Player(value)
    }
}

impl From<NonPlayerUpdate> for NetworkUpdate {
    fn from(value: NonPlayerUpdate) -> Self {
        NetworkUpdate::NonPlayer(value)
    }
}

/// One player-presence view: a crdt store of foreign players plus the channels feeding
/// it (from transports) and fanning it out (to scenes). The client has exactly one (the
/// shared context); an orchestrated multi-tenant server spawns one per scene room, so a
/// scene can only ever observe players connected to its own room — packets cannot reach
/// another context's store because routing is fixed when a transport binds to a context.
#[derive(Component)]
pub struct GlobalCrdtState {
    /// scene-room hash this context serves; `None` = the shared context
    pub room: Option<String>,
    // receiver from sockets
    ext_receiver: mpsc::Receiver<NetworkUpdate>,
    // sender for sockets to post to
    ext_sender: mpsc::Sender<NetworkUpdate>,
    // sender for broadcast updates
    int_sender: broadcast::Sender<GlobalCrdtStateUpdate>,
    allocator: PlayerIdAllocator,
    store: CrdtStore,
    lookup: BiMap<Address, Entity>,
    pub(crate) realm_bounds: (IVec2, IVec2),
    // per-component localizer registry (populated as components are first sent)
    localizers: HashMap<SceneComponentId, Localizer>,
}

impl GlobalCrdtState {
    pub fn new(room: Option<String>) -> Self {
        let (ext_sender, ext_receiver) = mpsc::channel(1000);
        let (int_sender, _) = broadcast::channel(1000);
        Self {
            room,
            ext_receiver,
            ext_sender,
            int_sender,
            allocator: Default::default(),
            store: Default::default(),
            lookup: Default::default(),
            realm_bounds: (IVec2::MAX, IVec2::MIN),
            localizers: Default::default(),
        }
    }
}

/// Resolves the network-update sender of the crdt context a given transport feeds —
/// the only route by which transport-scoped systems may inject player updates.
#[derive(bevy::ecs::system::SystemParam)]
pub struct TransportSenders<'w, 's> {
    transports: Query<'w, 's, &'static Transport>,
    contexts: Query<'w, 's, &'static GlobalCrdtState>,
}

impl TransportSenders<'_, '_> {
    pub fn get(&self, transport: Entity) -> Option<mpsc::Sender<NetworkUpdate>> {
        let transport = self.transports.get(transport).ok()?;
        Some(self.contexts.get(transport.context).ok()?.get_sender())
    }
}

/// Index of live crdt contexts: scene-room hash (`""` = the shared context; scene hashes
/// are never empty) → the entity holding its `GlobalCrdtState`. The shared context
/// always exists; room contexts are spawned/despawned by the orchestrated server
/// alongside their scene rooms.
#[derive(Resource, Default)]
pub struct CrdtContexts(pub HashMap<String, Entity>);

/// The realm each hosted scene belongs to, by scene hash. Only an orchestrated server populates it,
/// from the `realm` its orchestrator states per `add-scene`, and only it needs to: that is the one
/// deployment where scenes come from several realms at once (one server cohosting
/// `cozyfarm.dcl.eth` and `towerofmadness.dcl.eth`), so `CurrentRealm` cannot answer the question.
/// Everywhere else — a client, a standalone preview server — it stays empty and the realm the
/// process is on is the answer for every scene.
#[derive(Resource, Default)]
pub struct SceneRealms(pub HashMap<String, String>);

impl SceneRealms {
    /// The realm to announce for a scene: the one its orchestrator stated, or `default` — the realm
    /// this process is on, which is the answer for every deployment that isn't orchestrated. `None`
    /// when neither applies: an orchestrated engine is on no realm of its own, so a scene it was
    /// handed without one has no realm to fall back to.
    pub fn for_scene_hash(&self, hash: &str, default: Option<&str>) -> Option<String> {
        self.0
            .get(hash)
            .cloned()
            .or_else(|| default.map(str::to_owned))
    }
}

impl CrdtContexts {
    pub fn shared(&self) -> Entity {
        *self.0.get("").expect("shared crdt context missing")
    }

    /// The context a scene with the given hash should use. On the client (and on a
    /// standalone single-scene server) room contexts never exist, so every scene
    /// resolves to the single shared context. On a multi-tenant server every scene's
    /// room context is registered before the scene is queued and lives until the scene
    /// is gone, so a miss is an ordering bug — and falling back to the shared context
    /// would silently cross-contaminate room presence, so panic instead.
    pub fn for_scene_hash(&self, hash: &str) -> Entity {
        self.try_for_scene_hash(hash)
            .unwrap_or_else(|| panic!("no crdt context for scene {hash} on a multi-tenant server"))
    }

    /// [`Self::for_scene_hash`] without the panic, for callers that sweep every loaded scene rather
    /// than acting on one being queued: on a multi-tenant server a scene whose context isn't
    /// registered yet is simply not ready to be routed to, not a bug.
    pub fn try_for_scene_hash(&self, hash: &str) -> Option<Entity> {
        self.0
            .get(hash)
            .copied()
            .or_else(|| (!common::structs::multi_tenant()).then(|| self.shared()))
    }
}

impl GlobalCrdtState {
    // get a channel to which updates can be sent
    pub fn get_sender(&self) -> mpsc::Sender<NetworkUpdate> {
        self.ext_sender.clone()
    }

    /// Get a clone of the current CRDT store (with position data localized for the given
    /// scene origin) and a channel from which future updates can be received.
    pub fn subscribe(
        &self,
        scene_origin: bevy::prelude::Vec3,
    ) -> (CrdtStore, broadcast::Receiver<GlobalCrdtStateUpdate>) {
        let mut store = self.store.clone();
        let origin = SceneOrigin(scene_origin);

        // Localize position-containing entries in the initial store snapshot
        for (component_id, localizer) in &self.localizers {
            if matches!(localizer, Localizer::None | Localizer::Unimplemented) {
                continue;
            }
            if let Some(lww_state) = store.lww.get_mut(component_id) {
                for entry in lww_state.last_write.values_mut() {
                    if entry.is_some && !entry.data.is_empty() {
                        entry.data = localizer.localize_payload(&entry.data, &origin);
                    }
                }
            }
        }

        (store, self.int_sender.subscribe())
    }

    pub fn set_bounds(&mut self, min: IVec2, max: IVec2) {
        info!("bounds: {min}-{max}");
        self.realm_bounds = (min, max);
    }

    pub fn update_crdt<T: GlobalCrdtData>(
        &mut self,
        component_id: SceneComponentId,
        crdt_type: CrdtType,
        id: SceneEntityId,
        data: &T,
    ) {
        let localizer = T::localizer();
        assert!(
            matches!(crdt_type, CrdtType::LWW(_)) || matches!(localizer, Localizer::None),
            "GO components with explicit localization are not supported"
        );
        if !matches!(localizer, Localizer::None) {
            self.localizers
                .entry(component_id)
                .or_insert_with(|| localizer.clone());
        }

        let mut buf = Vec::new();
        DclWriter::new(&mut buf).write(data);
        let timestamp =
            self.store
                .force_update(component_id, crdt_type, id, Some(&mut DclReader::new(&buf)));
        let crdt_message = match crdt_type {
            CrdtType::LWW(_) => put_component(&id, &component_id, &timestamp, Some(&buf)),
            CrdtType::GO(_) => append_component(&id, &component_id, &buf),
        };
        self.send_update(
            GlobalCrdtStateUpdate::Crdt(crdt_message, localizer),
            "foreign player",
        );
    }

    pub fn delete_entity(&mut self, id: SceneEntityId) {
        self.store.clean_up(&HashSet::from_iter(Some(id)));
        let crdt_message = delete_entity(&id);
        self.send_update(
            GlobalCrdtStateUpdate::Crdt(crdt_message, Localizer::None),
            "foreign player",
        );
    }

    pub fn update_time(&mut self, time: f32) {
        self.send_update(GlobalCrdtStateUpdate::Time(time), "time");
    }

    pub fn update_camera_fov(&mut self, fov_y: f32) {
        self.send_update(GlobalCrdtStateUpdate::CameraFov(fov_y), "camera fov");
    }

    // a broadcast send fails exactly when there are no subscribers, which is the
    // normal state whenever no scenes are live — not an error, just nobody listening
    fn send_update(&self, update: GlobalCrdtStateUpdate, what: &str) {
        if self.int_sender.receiver_count() == 0 {
            return;
        }
        if let Err(e) = self.int_sender.send(update) {
            error!("failed to send {what} update to scenes: {e}");
        }
    }
}

#[derive(Component, Debug)]
pub struct ForeignPlayer {
    pub address: Address,
    /// the crdt context (player-presence view) this player entity belongs to; the same
    /// wallet connected to several rooms is a separate entity in each room's context
    pub context: Entity,
    /// Transports this player is currently connected through. Membership is implied
    /// by receiving data; transports report explicit departures (`NetworkUpdate::PlayerLeft`)
    /// and transport despawn removes the entity from every set. Empty set = disconnected.
    pub transports: HashSet<Entity>,
    pub scene_id: SceneEntityId,
    pub profile_version: u32,
    audio_sender: mpsc::Sender<ForeignAudioData>,
}

pub enum ChannelControl {
    VoiceSubscribe(
        Address,
        oneshot::Sender<StreamingSoundData<AudioDecoderError>>,
    ),
    VoiceUnsubscribe(Address),
}

pub enum ForeignAudioData {
    TransportAvailable(Entity),
    TransportUnavailable(Entity),
}

#[derive(Component)]
pub struct ForeignAudioSource {
    audio_available_receiver: mpsc::Receiver<ForeignAudioData>,
    available_transports: HashSet<Entity>,
    pub current_transport: Option<Entity>,
    pub audio_receiver: Option<oneshot::Receiver<StreamingSoundData<AudioDecoderError>>>,
}

#[derive(Clone)]
pub struct LocalAudioFrame {
    pub data: Arc<[i16]>,
    pub sample_rate: u32,
    pub num_channels: u32,
    pub samples_per_channel: u32,
}

#[derive(Resource)]
pub struct LocalAudioSource {
    pub sender: tokio::sync::broadcast::Sender<LocalAudioFrame>,
}

impl LocalAudioSource {
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<LocalAudioFrame> {
        self.sender.subscribe()
    }
}

#[derive(Serialize, Deserialize, Component, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ForeignMetaData {
    pub lambdas_endpoint: String,
}
/// A positional update for a foreign player from the Pulse-decoded movement stream. Position only;
/// scene-driven animation travels separately as [`PlayerSceneAnimEvent`].
#[derive(Event)]
pub struct PlayerPositionEvent {
    pub player: Entity,
    /// Local receive time (`time.elapsed_secs()`) — drives the interpolation catch-up window.
    pub time: f32,
    /// The source clock for this update, in seconds: the Pulse server tick, or the sender's own
    /// clock on the LiveKit path. Only ever compared against a previous value from the same
    /// source, to reject reordered/duplicate packets.
    ///
    /// `f64` because a Pulse tick is absolute milliseconds (~2.4e9). In `f32` that has a 256ms
    /// quantum, so consecutive deltas collapse onto one value and the strictly-newer gate drops
    /// them — which stranded foreign avatars mid-slide when the update that zeroed their velocity
    /// was discarded.
    pub timestamp: f64,
    pub translation: DclTranslation,
    pub rotation: DclQuat,
    pub velocity: Vec3,
    pub grounded: Option<bool>,
    /// MoveKind inferred from the packet:
    ///   - `jump_count >= 2` → `DoubleJump`
    ///   - `glide_state` is `OPENING_PROP` or `GLIDING` → `Glide`
    ///   - `is_jumping` or `is_long_jump` → `Jump`
    ///
    /// `None` means no movement-state indicator — the velocity fallback picks. Drives `jump_time`
    /// (Jump) and the matching emote (DoubleJump / Glide) on foreign avatars.
    pub remote_move_kind: Option<MoveKind>,
    /// This update is a discontinuous reposition (a Pulse `TeleportPerformed`), so `foreign_dynamics`
    /// snaps straight to it instead of interpolating across the gap.
    pub teleport: bool,
    /// Per-axis ± box the true position lies within (the position quantization half-step). `ZERO`
    /// for precise sources (LiveKit, full-state snapshots); set only by the Pulse delta receiver,
    /// where position arrives quantized. `foreign_dynamics` dead-reckons inside this box instead of
    /// snapping to `translation`, so a slowly-moving peer doesn't jitter on the quantization grid.
    pub precision: Vec3,
}

/// Scene-driven animation state for a foreign player, decoded from the standalone
/// `SceneDrivenAnimation` packet (LiveKit). `time` is the local receive time; `foreign_dynamics`
/// holds the clip for the foreign-position interpolation lag from there so it lands with the
/// visible (delayed) avatar rather than the freshly-arrived data.
#[derive(Event)]
pub struct PlayerSceneAnimEvent {
    pub player: Entity,
    pub time: f32,
    /// Resolved animation; `None` clears any active one.
    pub anim: Option<SceneDrivenAnimationRequest>,
    /// Render-only avatar lean (pitch, roll) in degrees, composed onto the interpolated yaw in
    /// `foreign_dynamics`. `(0, 0)` is upright.
    pub tilt: (f32, f32),
}

pub enum ProfileEventType {
    Request {
        request: rfc4::ProfileRequest,
        /// The transport the request arrived on — where the response is sent. A peer only
        /// reaches us over a transport we share, so this is the one channel we know works,
        /// rather than a guess made when some earlier message happened to arrive.
        transport: Entity,
    },
    Version(rfc4::AnnounceProfileVersion),
    Response(rfc4::ProfileResponse),
}

#[derive(Event)]
pub struct ProfileEvent {
    pub sender: Entity,
    pub event: ProfileEventType,
}

#[derive(Event, Debug)]
pub struct ChatEvent {
    pub timestamp: f64,
    pub sender: Entity,
    pub channel: String,
    pub message: String,
}

#[derive(Default, Resource, Deref, DerefMut)]
pub struct VoiceMessageStreams {
    streams: Vec<RpcStreamSender<VoiceMessage>>,
}

/// Per-player scene-driven-animation receive state, bundled into one [`Local`] to stay under
/// Bevy's system-parameter limit.
#[derive(Default)]
pub struct RemoteAnimState {
    /// Cached hash pair + URN per player, so ride-along packets (which omit the hashes between
    /// keepalives) keep resolving without rebuilding.
    cache: HashMap<Entity, CachedRemoteAnim>,
    /// Highest SDA sequence seen per player, to drop reordered/duplicate datagrams (unreliable).
    last_sequence: HashMap<Entity, u32>,
}

/// Apply a foreign player's [`rfc4::Movement`], whether it arrived over Pulse (as
/// [`PlayerMessage::Movement`]) or as a legacy rfc4 packet over a websocket transport (as
/// [`PlayerMessage::PlayerData`]). Writes the CRDT transform and emits a [`PlayerPositionEvent`]
/// for `foreign_dynamics` to interpolate.
#[allow(clippy::too_many_arguments)]
fn apply_foreign_movement(
    m: &rfc4::Movement,
    entity: Entity,
    scene_id: SceneEntityId,
    now: f32,
    timestamp: f64,
    teleport: bool,
    commands: &mut Commands,
    state: &mut GlobalCrdtState,
    position_events: &mut EventWriter<PlayerPositionEvent>,
) {
    debug!("movement data: {m:?}");
    commands.entity(entity).try_insert((
        HeadSync {
            yaw_deg: m.head_yaw,
            pitch_deg: m.head_pitch,
            yaw_enabled: m.head_ik_yaw_enabled,
            pitch_enabled: m.head_ik_pitch_enabled,
        },
        PointAtSync {
            target_world: Vec3::new(m.point_at_x, m.point_at_y, m.point_at_z),
            is_pointing: m.is_pointing_at,
        },
    ));
    let pos = Vec3::new(m.position_x, m.position_y, -m.position_z);
    let vel = Vec3::new(m.velocity_x, m.velocity_y, -m.velocity_z);
    // Yaw only — the render-only lean is no longer composed here. It rides the
    // SceneDrivenAnimation packet and is applied in `foreign_dynamics`, so the
    // transform other clients read (and this CRDT entry) stays the bare yaw.
    let rot = Quat::from_rotation_y(-m.rotation_y / 360.0 * TAU);
    let dcl_transform = DclTransformAndParent {
        translation: DclTranslation::from_bevy_translation(pos),
        rotation: DclQuat::from_bevy_quat(rot),
        scale: Vec3::ONE,
        parent: SceneEntityId::WORLD_ORIGIN,
    };

    state.update_crdt(
        SceneComponentId::TRANSFORM,
        CrdtType::LWW_ANY,
        scene_id,
        &dcl_transform,
    );
    // Glide is checked before DoubleJump because Unity keeps `jump_count`
    // at its last value (usually 2) through the whole glide — so the
    // DoubleJump-first ordering would mask an active glide.
    let remote_move_kind = match (
        m.glide_state(),
        m.jump_count,
        m.is_jumping || m.is_long_jump,
    ) {
        (rfc4::movement::GlideState::OpeningProp | rfc4::movement::GlideState::Gliding, _, _) => {
            Some(MoveKind::Glide)
        }
        (_, c, _) if c >= 2 => Some(MoveKind::DoubleJump),
        (_, _, true) => Some(MoveKind::Jump),
        _ => None,
    };
    // Set only by the Pulse delta receiver; absent (→ ZERO, exact) for LiveKit and full-state.
    // A symmetric ± box, so the z-axis negation above doesn't affect it.
    let precision = m
        .position_precision
        .as_ref()
        .map(|p| Vec3::new(p.x, p.y, p.z))
        .unwrap_or(Vec3::ZERO);
    position_events.write(PlayerPositionEvent {
        player: entity,
        time: now,
        timestamp,
        translation: dcl_transform.translation,
        rotation: dcl_transform.rotation,
        velocity: vel,
        grounded: Some(m.is_grounded),
        remote_move_kind,
        teleport,
        precision,
    });
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn process_transport_updates(
    mut commands: Commands,
    mut contexts: Query<(Entity, &mut GlobalCrdtState)>,
    mut players: Query<&mut ForeignPlayer>,
    time: Res<Time>,
    mut profile_events: EventWriter<ProfileEvent>,
    mut position_events: EventWriter<PlayerPositionEvent>,
    mut anim_events: EventWriter<PlayerSceneAnimEvent>,
    mut chat_events: EventWriter<ChatEvent>,
    mut string_senders: Local<HashMap<String, RpcEventSender>>,
    mut binary_senders: Local<HashMap<String, RpcStreamSender<(String, Vec<u8>)>>>,
    mut subscribers: EventReader<RpcCall>,
    mut profile_meta_cache: ResMut<ProfileMetaCache>,
    mut duplicate_chat_filter: Local<HashMap<Entity, f64>>,
    mut remote_anim: Local<RemoteAnimState>,
    discard_player_updates: Res<DiscardPlayerUpdates>,
) {
    // gather any event receivers
    for ev in subscribers.read() {
        match ev {
            RpcCall::SubscribeMessageBus { sender, hash } => {
                string_senders.insert(hash.clone(), sender.clone());
            }
            RpcCall::SubscribeBinaryBus { sender, hash } => {
                binary_senders.insert(hash.clone(), sender.clone());
            }
            _ => (),
        }
    }
    string_senders.retain(|_, s| !s.is_closed());
    binary_senders.retain(|_, s| !s.is_closed());

    // each context is fully independent: its own transports feed it, its own player
    // entities live in it, and only its own scenes observe it
    for (context_entity, mut state) in contexts.iter_mut() {
        let mut created_this_frame: HashMap<
            Address,
            (Entity, SceneEntityId, mpsc::Sender<ForeignAudioData>),
        > = HashMap::new();

        while let Ok(network_update) = state.ext_receiver.try_recv() {
            match network_update {
                NetworkUpdate::Player(update) => {
                    if discard_player_updates.0 {
                        continue;
                    }
                    // create/update timestamp/transport_id on the foreign player
                    let (entity, scene_id, audio_channel) =
                        if let Some((entity, scene_id, channel)) =
                            created_this_frame.get(&update.address)
                        {
                            (*entity, *scene_id, channel.clone())
                        } else if let Some(existing) = state.lookup.get_by_left(&update.address) {
                            let mut foreign_player = players.get_mut(*existing).unwrap();
                            foreign_player.transports.insert(update.transport_id);
                            (
                                *existing,
                                foreign_player.scene_id,
                                foreign_player.audio_sender.clone(),
                            )
                        } else {
                            let Some(next_free) = state.allocator.alloc() else {
                                warn!("no space for any more players!");
                                continue;
                            };

                            state.update_crdt(
                                SceneComponentId::PLAYER_IDENTITY_DATA,
                                CrdtType::LWW_ANY,
                                next_free,
                                &PbPlayerIdentityData {
                                    address: format!("{:#x}", update.address),
                                    is_guest: true,
                                },
                            );

                            let (audio_sender, audio_receiver) =
                                mpsc::channel::<ForeignAudioData>(10);

                            let new_entity = commands
                                .spawn((
                                    Transform::default(),
                                    Visibility::default(),
                                    ForeignPlayer {
                                        address: update.address,
                                        context: context_entity,
                                        transports: HashSet::from_iter([update.transport_id]),
                                        scene_id: next_free,
                                        profile_version: 0,
                                        audio_sender: audio_sender.clone(),
                                    },
                                    ForeignAudioSource {
                                        audio_available_receiver: audio_receiver,
                                        audio_receiver: None,
                                        available_transports: Default::default(),
                                        current_transport: None,
                                    },
                                    HeadSync::default(),
                                    PointAtSync::default(),
                                    Propagate(RenderLayers::default()),
                                ))
                                .id();

                            state.lookup.insert(update.address, new_entity);

                            info!(
                                "creating new player: {} -> {:?} / {}",
                                update.address, new_entity, next_free
                            );
                            created_this_frame.insert(
                                update.address,
                                (new_entity, next_free, audio_sender.clone()),
                            );
                            (new_entity, next_free, audio_sender)
                        };

                    // process update
                    match update.message {
                        // presence only — registering the transport above was the whole point
                        PlayerMessage::Joined => (),
                        PlayerMessage::MetaData(str) => {
                            if let Ok(meta) = serde_json::from_str::<ForeignMetaData>(&str) {
                                debug!("foreign player metadata: {scene_id:?}: {meta:?}");
                                profile_meta_cache
                                    .0
                                    .insert(update.address, meta.lambdas_endpoint);
                            }
                        }
                        PlayerMessage::AudioStreamAvailable { transport } => {
                            // pass through
                            debug!("{transport} available for {entity}!");
                            let _ = audio_channel
                                .try_send(ForeignAudioData::TransportAvailable(transport));
                        }
                        PlayerMessage::AudioStreamUnavailable { transport } => {
                            // pass through
                            debug!("{transport} not available for {entity}!");
                            let _ = audio_channel
                                .try_send(ForeignAudioData::TransportUnavailable(transport));
                        }
                        // rfc4 avatar state is not ingested from any byte transport: movement and
                        // emotes arrive over Pulse alone, as `PlayerMessage::Movement` /
                        // `PlayerMessage::Emote` below. Applying a byte-transport copy as well would
                        // double-drive the avatar, and the two clocks can't be reconciled — an rfc4
                        // timestamp is the sender's, a Pulse one is the server tick.
                        PlayerMessage::PlayerData(
                            Message::Position(_)
                            | Message::MovementCompressed(_)
                            | Message::Movement(_)
                            | Message::PlayerEmote(_),
                        ) => {}
                        PlayerMessage::PlayerData(Message::ProfileVersion(version)) => {
                            profile_events.write(ProfileEvent {
                                sender: entity,
                                event: ProfileEventType::Version(version),
                            });
                        }
                        PlayerMessage::PlayerData(Message::ProfileRequest(request)) => {
                            profile_events.write(ProfileEvent {
                                sender: entity,
                                event: ProfileEventType::Request {
                                    request,
                                    transport: update.transport_id,
                                },
                            });
                        }
                        PlayerMessage::PlayerData(Message::ProfileResponse(response)) => {
                            profile_events.write(ProfileEvent {
                                sender: entity,
                                event: ProfileEventType::Response(response),
                            });
                        }
                        PlayerMessage::PlayerData(Message::Chat(chat)) => {
                            let last = duplicate_chat_filter.entry(entity).or_default();

                            if *last < chat.timestamp {
                                debug!("chat data: `{chat:#?}`");
                                chat_events.write(ChatEvent {
                                    sender: entity,
                                    timestamp: chat.timestamp,
                                    channel: "Nearby".to_owned(),
                                    message: chat.message,
                                });
                                *last = chat.timestamp;
                            }
                        }
                        PlayerMessage::PlayerData(Message::Scene(scene)) => {
                            let address = format!("{:#x}", update.address);
                            if let Some(room_hash) = &state.room {
                                // a client may only address the scene owning this context's
                                // room — rejects bus injection via a forged scene_id
                                if scene.scene_id != *room_hash {
                                    debug!(
                                        "dropping cross-room bus message from {address}: declared scene {} != room {room_hash}",
                                        scene.scene_id
                                    );
                                    continue;
                                }
                            }
                            process_messagebus(
                                scene,
                                address,
                                &mut string_senders,
                                &mut binary_senders,
                            );
                        }
                        PlayerMessage::PlayerData(Message::Voice(_)) => (),
                        PlayerMessage::Movement {
                            movement,
                            teleport,
                            timestamp,
                        } => apply_foreign_movement(
                            &movement,
                            entity,
                            scene_id,
                            time.elapsed_secs(),
                            timestamp,
                            teleport,
                            &mut commands,
                            &mut state,
                            &mut position_events,
                        ),
                        PlayerMessage::Emote {
                            urn,
                            incremental_id,
                            stopping,
                        } => {
                            debug!("emote: {urn} (stopping: {stopping})");
                            if stopping {
                                // Explicit stop (a looping emote cancelled, or a one-shot's server
                                // completion). Foreign emotes no longer self-cancel on motion (see
                                // `animate`), so the wire stop is what ends a looping one.
                                commands.entity(entity).remove::<EmoteCommand>();
                            } else {
                                commands.entity(entity).try_insert(EmoteCommand {
                                    timestamp: incremental_id as i64,
                                    urn,
                                    r#loop: false,
                                });
                            }
                        }
                        PlayerMessage::PlayerData(Message::SceneDrivenAnimation(sda)) => {
                            // Standalone scene-driven animation (decoupled from movement). Order by the
                            // sender's monotonic sequence — LiveKit is unreliable, so drop reordered /
                            // duplicate datagrams. `foreign_dynamics` holds the resolved anim for the
                            // foreign-position interpolation lag so it lands with the visible avatar.
                            let sequence = sda.sequence();
                            if remote_anim
                                .last_sequence
                                .get(&entity)
                                .is_none_or(|&prev| sequence > prev)
                            {
                                remote_anim.last_sequence.insert(entity, sequence);
                                let tilt = (sda.tilt_pitch(), sda.tilt_roll());
                                let anim = resolve_remote_anim(
                                    entity,
                                    update.address,
                                    &mut remote_anim.cache,
                                    Some(sda),
                                );
                                anim_events.write(PlayerSceneAnimEvent {
                                    player: entity,
                                    time: time.elapsed_secs(),
                                    anim,
                                    tilt,
                                });
                            }
                        }
                        PlayerMessage::PlayerData(Message::SceneEmote(scene_emote)) => {
                            debug!("scene emote: {scene_emote:?}");
                        }
                    }
                }
                NetworkUpdate::NonPlayer(update) => {
                    if update.address != "authoritative-server" {
                        warn!(
                            "skipping unexpected update from {}: {:?}",
                            update.address, update.message
                        );
                        continue;
                    }

                    let Message::Scene(scene) = update.message else {
                        warn!(
                            "skipping unexpected update from {}: {:?}",
                            update.address, update.message
                        );
                        continue;
                    };

                    // same cross-room guard as the Player arm: the message may only
                    // address the scene owning this context's room
                    if let Some(room_hash) = &state.room {
                        if scene.scene_id != *room_hash {
                            debug!(
                                "dropping cross-room message from {}: declared scene {} != room {room_hash}",
                                update.address, scene.scene_id
                            );
                            continue;
                        }
                    }

                    process_messagebus(
                        scene,
                        update.address,
                        &mut string_senders,
                        &mut binary_senders,
                    );
                }
                NetworkUpdate::PlayerLeft {
                    transport_id,
                    address,
                } => {
                    if let Some(entity) = state.lookup.get_by_left(&address) {
                        debug!("player {address:#x} left transport {transport_id}");
                        if let Ok(mut foreign_player) = players.get_mut(*entity) {
                            foreign_player.transports.remove(&transport_id);
                        } else {
                            // players spawned earlier this frame aren't in the query yet
                            commands.entity(*entity).modify_component(
                                move |foreign_player: &mut ForeignPlayer| {
                                    foreign_player.transports.remove(&transport_id);
                                },
                            );
                        }
                    }
                }
            }
        }
    }
}

// Resolves a standalone `SceneDrivenAnimation` packet into the full state. An empty
// `scene_hash` clears the state; absent hash fields (None) reuse the last cached pair
// for this entity so we keep animating between keepalives. Returns `None` when the
// sender has no active scene-driven animation. The resolved state is emitted as a
// `PlayerSceneAnimEvent` so `foreign_dynamics` can apply it when the interpolated
// position reaches the SDA's server timestamp.
pub struct CachedRemoteAnim {
    scene_hash: String,
    content_hash: String,
    urn: String,
}

fn resolve_remote_anim(
    entity: Entity,
    sender: Address,
    last_anims: &mut HashMap<Entity, CachedRemoteAnim>,
    anim: Option<dcl_component::proto_components::kernel::comms::rfc4::SceneDrivenAnimation>,
) -> Option<SceneDrivenAnimationRequest> {
    // Sender didn't attach the nested carrier; nothing to do (and nothing to clear,
    // since we only cache hashes the sender itself told us about).
    let anim = anim?;

    // Guard against mirror-class bugs where a buggy remote re-emits someone else's
    // nested message byte-for-byte (observed against a Unity client that pooled
    // protobuf instances without discarding unknown fields). `origin_address` must be
    // present and match the packet sender; anything else is either a mirror or a
    // sender that predates this field and is therefore also potentially a mirror
    // victim. Be strict and drop it.
    let sender_str = format!("{sender:#x}");
    match anim.origin_address.as_deref() {
        Some(origin) if origin.eq_ignore_ascii_case(&sender_str) => {}
        _ => {
            debug!(
                "dropping scene_driven_animation without matching origin_address: origin={:?} sender={}",
                anim.origin_address, sender_str
            );
            last_anims.remove(&entity);
            return None;
        }
    }

    // Wire convention: on transition the sender ships both hashes (or an empty
    // scene_hash to clear); between transitions both are omitted and we re-apply the
    // cached entry (hashes + pre-built URN) so ride-along fields (speed, loop, seek)
    // keep updating without rebuilding the URN per packet.
    let (scene_hash, content_hash, urn) = match anim.scene_hash {
        Some(s) if s.is_empty() => {
            last_anims.remove(&entity);
            return None;
        }
        Some(s) => {
            let c = anim.content_hash?;
            let urn = format!("urn:decentraland:off-chain:scene-emote:{s}-{c}-false");
            last_anims.insert(
                entity,
                CachedRemoteAnim {
                    scene_hash: s.clone(),
                    content_hash: c.clone(),
                    urn: urn.clone(),
                },
            );
            (s, c, urn)
        }
        None => {
            let cached = last_anims.get(&entity)?;
            (
                cached.scene_hash.clone(),
                cached.content_hash.clone(),
                cached.urn.clone(),
            )
        }
    };

    let speed = anim.speed?;
    let r#loop = anim.r#loop.unwrap_or(false);
    let transition_seconds = anim.transition_seconds.unwrap_or(0.2);

    Some(SceneDrivenAnimationRequest {
        src: String::new(),
        urn,
        scene_hash,
        content_hash,
        r#loop,
        speed,
        // Foot-IK on remote avatars gates on this — without it, remotes never apply
        // foot-IK to a scene-driven animation. Defaults to false if the sender
        // predates the field, which matches the conservative pre-field behaviour.
        idle: anim.idle.unwrap_or(false),
        transition_seconds,
        seek: anim.playback_time,
        sounds: anim.sound_content_hashes,
    })
}

fn process_messagebus(
    mut scene: rfc4::Scene,
    address: String,
    string_senders: &mut HashMap<String, RpcStreamSender<String>>,
    binary_senders: &mut HashMap<String, RpcStreamSender<(String, Vec<u8>)>>,
) {
    if scene.data.is_empty() {
        warn!("empty scene message");
        return;
    }

    let comms_type = match *scene.data.first().unwrap() {
        c if c == CommsMessageType::String as u8 => {
            scene.data.remove(0);
            CommsMessageType::String
        }
        c if c == CommsMessageType::Binary as u8 => {
            scene.data.remove(0);
            CommsMessageType::Binary
        }
        _ => CommsMessageType::String,
    };

    debug!(
        "messagebus received from {} to scene {}: [{:?}] `{:?}`",
        address, scene.scene_id, comms_type, scene.data
    );

    match comms_type {
        CommsMessageType::String => {
            if let Some(sender) = string_senders.get(&scene.scene_id) {
                let _ = sender.send(
                    json!({
                        "message": String::from_utf8(scene.data).unwrap_or_default(),
                        "sender": address,
                    })
                    .to_string(),
                );
            }
        }
        CommsMessageType::Binary => {
            if let Some(sender) = binary_senders.get(&scene.scene_id) {
                let _ = sender.send((address, scene.data));
            }
        }
    }
}

fn despawn_players(
    mut commands: Commands,
    players: Query<(Entity, &ForeignPlayer)>,
    mut contexts: Query<&mut GlobalCrdtState>,
) {
    for (entity, player) in players.iter() {
        if player.transports.is_empty() {
            if let Ok(mut commands) = commands.get_entity(entity) {
                info!("removing disconnected player: {entity:?} : {player:?}");
                commands.despawn();
            }

            // context may already be gone if its scene room was torn down
            if let Ok(mut state) = contexts.get_mut(player.context) {
                state.delete_entity(player.scene_id);
                state.allocator.free(player.scene_id);
                state.lookup.remove_by_right(&entity);
            }
        }
    }
}

/// A despawned context (scene room torn down) takes its player entities with it.
fn remove_context_players(
    trigger: Trigger<OnReplace, GlobalCrdtState>,
    players: Query<(Entity, &ForeignPlayer)>,
    mut commands: Commands,
) {
    let context = trigger.target();
    for (entity, player) in players.iter() {
        if player.context == context {
            info!(
                "removing player {:#x} with despawned context {context}",
                player.address
            );
            commands.entity(entity).try_despawn();
        }
    }
}

fn remove_transport_from_foreign_players(
    trigger: Trigger<OnReplace, Transport>,
    mut players: Query<&mut ForeignPlayer>,
) {
    let transport_id = trigger.target();

    for mut player in players.iter_mut() {
        player.transports.remove(&transport_id);
    }
}

fn remove_transport_from_foreign_audio_source(
    trigger: Trigger<OnReplace, Transport>,
    foreign_audio_sources: Query<&mut ForeignAudioSource>,
) {
    let entity = trigger.target();

    for mut foreign_audio_source in foreign_audio_sources {
        foreign_audio_source.available_transports.remove(&entity);
    }
}

fn handle_foreign_audio(
    transports: Query<(Entity, &Transport)>,
    mut q: Query<(&mut ForeignAudioSource, &ForeignPlayer)>,
) {
    let transports = transports
        .iter()
        .filter_map(|(e, transport)| transport.control.as_ref().map(|t| (e, t)))
        .collect::<HashMap<_, _>>();

    for (mut source, player) in q.iter_mut() {
        let prev_available = source.available_transports.clone();
        let prev_transport = source.current_transport;

        // handle publish/unpublish
        while let Ok(event) = source.audio_available_receiver.try_recv() {
            match event {
                ForeignAudioData::TransportAvailable(entity) => {
                    source.available_transports.insert(entity);
                }
                ForeignAudioData::TransportUnavailable(entity) => {
                    source.available_transports.remove(&entity);
                }
            }
        }

        // validate available transports
        source
            .available_transports
            .retain(|t| transports.contains_key(t));

        // validate current source
        if source
            .current_transport
            .is_some_and(|current| !source.available_transports.contains(&current))
        {
            source.current_transport = None;
            source.audio_receiver = None;
        }

        // request a new source
        if source.current_transport.is_none() {
            if let Some(entity) = source.available_transports.iter().next() {
                let control = transports.get(entity).unwrap();
                let (sx, rx) = oneshot::channel();
                if let Ok(()) = control.try_send(ChannelControl::VoiceSubscribe(player.address, sx))
                {
                    source.current_transport = Some(*entity);
                    source.audio_receiver = Some(rx);
                }
            }
        }

        if source.available_transports != prev_available {
            debug!(
                "available: {:?} -> {:?}",
                prev_available, source.available_transports
            );
        }
        if source.current_transport != prev_transport {
            debug!(
                "current: {:?} -> {:?}",
                prev_transport, source.current_transport
            );
        }
    }
}

fn drop_closed_voice_message_senders(mut voice_message_streams: ResMut<VoiceMessageStreams>) {
    voice_message_streams.retain(|vms| !vms.is_closed());
}

fn receive_new_voice_message_senders(
    mut event_reader: EventReader<SystemApi>,
    mut voice_message_streams: ResMut<VoiceMessageStreams>,
) {
    for event in event_reader.read() {
        if let SystemApi::GetVoiceStream(stream) = event {
            voice_message_streams.push(stream.clone());
        }
    }
}
