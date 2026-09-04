use std::{collections::VecDeque, f32::consts::TAU, sync::Arc};

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
    util::{ModifyComponentExt, TaskExt},
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
        sdk::components::{
            common::AvatarMask, EmoteState, PbAvatarEmoteCommand, PbPlayerIdentityData,
        },
    },
    transform_and_parent::{DclQuat, DclTransformAndParent, DclTranslation},
    DclReader, DclWriter, GlobalCrdtData, Localizer, SceneComponentId, SceneEntityId, SceneOrigin,
};

use ipfs::{ActiveEntitiesRequest, ActiveEntityTask, EntityDefinition, IpfsAssetServer};

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
        app.add_event::<ForeignEmoteEvent>();
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
        /// On a stop: the server's one-shot timer expired (a natural finish) rather than the
        /// player cancelling a looping emote. Always false on a start.
        completed: bool,
        /// On a start: the `AvatarMask` value the server relayed, if the emote is partial-body.
        mask: Option<i32>,
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
                completed,
                mask,
            } => f
                .debug_struct("Emote")
                .field("urn", urn)
                .field("incremental_id", incremental_id)
                .field("stopping", stopping)
                .field("completed", completed)
                .field("mask", mask)
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
    /// Local receive time (`time.elapsed_secs_f64()`) — drives the interpolation catch-up window.
    pub time: f64,
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
    pub time: f64,
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
    now: f64,
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
                            time.elapsed_secs_f64(),
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
                            completed,
                            mask,
                        } => {
                            debug!("emote: {urn} (stopping: {stopping})");
                            if stopping {
                                // Explicit stop (a looping emote cancelled, or a one-shot's server
                                // completion). Foreign emotes no longer self-cancel on motion (see
                                // `animate`), so the wire stop is what ends a looping one.
                                commands.entity(entity).remove::<EmoteCommand>();
                                commands.send_event(ForeignEmoteEvent {
                                    player: entity,
                                    kind: ForeignEmoteEventKind::Stopped { completed },
                                });
                            } else {
                                commands.entity(entity).try_insert(EmoteCommand {
                                    timestamp: incremental_id as i64,
                                    urn: urn.clone(),
                                    r#loop: false,
                                });
                                commands.send_event(ForeignEmoteEvent {
                                    player: entity,
                                    kind: ForeignEmoteEventKind::Started { urn, mask },
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
                                    time: time.elapsed_secs_f64(),
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

                    match update.message {
                        Message::Scene(scene) => {
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
                        // a server resolving a guest profile asks the guest directly — the
                        // only source there is. The request handler reads the requested
                        // address and the reply transport, never the sender entity, so the
                        // transport entity stands in for the (playerless) server.
                        Message::ProfileRequest(request) => {
                            profile_events.write(ProfileEvent {
                                sender: update.transport_id,
                                event: ProfileEventType::Request {
                                    request,
                                    transport: update.transport_id,
                                },
                            });
                        }
                        message => {
                            warn!(
                                "skipping unexpected update from {}: {:?}",
                                update.address, message
                            );
                        }
                    }
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

// ---------------------------------------------------------------------------
// Foreign emote relay (render-free)
// ---------------------------------------------------------------------------

/// A remote player's emote lifecycle as decoded from the wire. Emitted by
/// [`process_transport_updates`] next to the `EmoteCommand` it maintains on the player entity, so
/// a consumer sees every transition in order — including a start and stop landing in one frame,
/// which the component alone would collapse — and gets what the component cannot carry: why an
/// emote stopped, and its body mask.
#[derive(Event, Debug, Clone, PartialEq)]
pub struct ForeignEmoteEvent {
    pub player: Entity,
    pub kind: ForeignEmoteEventKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ForeignEmoteEventKind {
    Started {
        urn: String,
        /// `AvatarMask` value from the server, if the emote is partial-body.
        mask: Option<i32>,
    },
    Stopped {
        /// The server's one-shot timer expired (a natural finish) rather than the player cancelling.
        completed: bool,
    },
}

/// Reports remote players' emotes to scenes as `AvatarEmoteCommand` entries — a grow-only set on
/// the player's entity — without any avatar rendering. Headless / authoritative-server only.
///
/// On the client this is `avatar::animate`'s job, and it reports only starts, once the clip has
/// loaded. A server loads no avatar assets, so nothing there ever wrote the component and a server
/// scene could never react to what players do. This relays the wire events instead, and reports the
/// whole lifecycle the component can express:
///
/// - a start is `ES_STARTED` with the emote's real `loop`: a scene emote carries it in the urn's
///   trailing `-{loop}` token, the reference client's embedded animations come from a fixed table,
///   and everything else — base and wearable emotes — is looked up in the emote's deployed
///   metadata (`emoteDataADR74.loop`) on the catalyst, cached by pointer. A start whose metadata
///   is still in flight waits, and so does everything queued behind it for that player, so a scene
///   always sees a player's events in wire order. A lookup that fails answers `false` for a while
///   and is then retried; an identifier no catalyst could resolve reports `false` at once.
/// - a stop is `ES_FINISHED` when the server's one-shot timer expired and `ES_INTERRUPTED` when
///   the player cancelled a looping emote, echoing the started emote's urn and loop so a scene can
///   match it without bookkeeping. A stop with no start on record is dropped.
/// - `timestamp` is a host-side monotonic counter. The SDK's grow-only set sorts a row by it and
///   trims from the front, so a non-monotonic key — the peer's clock, or the Pulse start tick of a
///   replayed emote — could see an entry dropped outright.
/// - `mask` passes through from the wire.
///
/// The urn is a peer-controlled string headed for every subscribed scene's crdt, so it is bounded
/// before it gets there; each player's queue, the pointer cache, the lookup wait queue and the
/// requests in flight are bounded too. A full player queue force-resolves its blocked head as a
/// one-shot rather than parting a start from its stop; a full wait queue reports a start as a
/// one-shot at once, uncached.
///
/// Deliberately not part of [`GlobalCrdtPlugin`]: the client must not report twice.
pub struct ForeignEmoteRelayPlugin;

impl Plugin for ForeignEmoteRelayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ForeignEmoteRelay>();
        // flush before resolve so a pointer first needed this frame is requested this frame
        app.add_systems(
            Update,
            (
                queue_foreign_emote_reports,
                flush_foreign_emote_reports,
                resolve_foreign_emote_metadata,
            )
                .chain()
                .after(process_transport_updates),
        );
    }
}

/// Longest emote identifier accepted from a peer.
const MAX_EMOTE_URN_BYTES: usize = 256;
/// Lifecycle reports one player may have waiting on a metadata lookup. Only a start awaiting its
/// `loop` can hold a queue, so at this size that head is reported as a one-shot instead and the
/// queue drains; nothing is dropped, so a start is never parted from its stop. Past twice this —
/// a single-frame burst from one peer, which Pulse rate-limits — the oldest go.
const MAX_PENDING_PER_PLAYER: usize = 16;
/// Resolved `loop` flags kept, by pointer. Emote collections are large, but a room's players use a
/// small working set.
const MAX_CACHED_POINTERS: usize = 1024;
/// How long a failed lookup answers `false` before the pointer is looked up again.
const LOOKUP_RETRY_SECS: f64 = 60.0;
/// Metadata requests in flight at once; further unknown pointers wait for a slot.
const MAX_LOOKUP_BATCHES: usize = 4;
/// Pointers per metadata request.
const MAX_LOOKUP_POINTERS_PER_REQUEST: usize = 32;
/// Pointers waiting for a request slot. Pointers derive from peer-controlled urns, so a burst of
/// unique ones must not grow memory or request size without bound: past this a start is reported
/// as a one-shot at once, uncached, rather than queued.
const MAX_WANTED_POINTERS: usize = 256;

const SCENE_EMOTE_PREFIX: &str = "urn:decentraland:off-chain:scene-emote:";
const BASE_EMOTE_PREFIX: &str = "urn:decentraland:off-chain:base-emotes:";

/// The reference client's embedded animation set (`EmbeddedEmotes.asset`), triggered by bare id
/// and never deployed to a catalyst, so a lookup could not resolve them. Only the sitting
/// animations loop.
const EMBEDDED_LOOPING_EMOTES: [&str; 4] = [
    "sittingchair1",
    "sittingchair2",
    "sittingground1",
    "sittingground2",
];
const EMBEDDED_ONESHOT_EMOTES: [&str; 17] = [
    "crafting",
    "handsintheair",
    "victory",
    "waving",
    "buttondown",
    "buttonfront",
    "gethit",
    "knockout",
    "lever",
    "openchest",
    "opendoor",
    "punch",
    "push",
    "swingweapononehand",
    "swingweapontwohands",
    "throw",
    "fistpump_short",
];

/// Where emote entities are deployed. Wearables and emotes live on the catalyst network, not on a
/// world or preview realm's content server, so the lookup goes there regardless of realm.
fn emote_catalyst_url() -> String {
    common::base_domain::https("peer", "/content")
}

/// How an emote's `loop` is known, decided from the identifier alone.
#[derive(Debug, Clone, PartialEq)]
enum LoopSource {
    /// Known without a lookup.
    Known(bool),
    /// Needs the deployed metadata registered under this catalyst pointer.
    Pointer(String),
}

/// Reject what no real emote identifier looks like: empty, oversized, or carrying whitespace /
/// control characters (urns, legacy names and embedded ids are all single tokens). Not a charset
/// allowlist: a false reject would silently drop a legitimate emote.
fn foreign_emote_urn_is_acceptable(urn: &str) -> bool {
    !urn.is_empty()
        && urn.len() <= MAX_EMOTE_URN_BYTES
        && !urn.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// The key a collectible pointer is requested and cached under: each `:` segment lowercased except
/// `b64-` ones, whose base64 payload is case-sensitive (as `CollectibleUrn` does). Applied to the
/// pointer derived from a peer's urn AND to the pointers a catalyst answers with, so the two meet.
fn canonical_pointer(pointer: &str) -> String {
    pointer
        .split(':')
        .map(|segment| {
            if segment.starts_with("b64-") {
                segment.to_owned()
            } else {
                segment.to_ascii_lowercase()
            }
        })
        .collect::<Vec<_>>()
        .join(":")
}

/// A relayed mask is written to a scene only if it names a value of the generated `AvatarMask`;
/// anything else would reach scene code as an unrecognised enum. (The generated enum offers no
/// `TryFrom<i32>`, so the variants are listed.)
fn valid_avatar_mask(mask: i32) -> bool {
    [AvatarMask::AmUpperBody as i32].contains(&mask)
}

/// Classify an emote identifier: a scene emote carries its loop flag, an embedded id is tabled, a
/// bare legacy name is a base emote, and a collectible urn is shortened to the pointer its deployed
/// entity is registered under (token id dropped, lowercased apart from `b64-` segments, as
/// `CollectibleUrn` does). `None`: nothing a catalyst could resolve.
fn classify_emote(urn: &str) -> Option<LoopSource> {
    if let Some(scene_emote) = urn.strip_prefix(SCENE_EMOTE_PREFIX) {
        let loops = scene_emote
            .rsplit_once('-')
            .is_some_and(|(_, flag)| flag.eq_ignore_ascii_case("true"));
        return Some(LoopSource::Known(loops));
    }
    let lowered = urn.to_ascii_lowercase();
    if EMBEDDED_LOOPING_EMOTES.contains(&lowered.as_str()) {
        return Some(LoopSource::Known(true));
    }
    if EMBEDDED_ONESHOT_EMOTES.contains(&lowered.as_str()) {
        return Some(LoopSource::Known(false));
    }
    if !urn.contains(':') {
        // legacy bare base-emote name ("wave"), still emitted by older clients and scene calls
        return Some(LoopSource::Pointer(format!("{BASE_EMOTE_PREFIX}{lowered}")));
    }
    let parts: Vec<&str> = urn.split(':').collect();
    let pointer_parts = match parts.get(3).copied() {
        Some("base-avatars" | "base-emotes") => 5,
        Some("collections-v1" | "collections-v2") => 6,
        Some("collections-thirdparty") => 7,
        _ => return None,
    };
    if parts.len() < pointer_parts {
        return None;
    }
    Some(LoopSource::Pointer(canonical_pointer(
        &parts[..pointer_parts].join(":"),
    )))
}

/// The `loop` flag in a deployed emote entity's metadata.
fn loop_from_emote_metadata(metadata: &serde_json::Value) -> Option<bool> {
    metadata
        .get("emoteDataADR74")
        .or_else(|| metadata.get("data"))
        .and_then(|data| data.get("loop"))
        .and_then(serde_json::Value::as_bool)
}

/// A lifecycle report waiting to be written, in wire order per player.
#[derive(Debug)]
enum PendingReport {
    Start {
        urn: String,
        mask: Option<i32>,
        source: LoopSource,
    },
    Stop {
        completed: bool,
    },
}

#[derive(Debug, Clone, Copy)]
struct CachedLoop {
    loops: bool,
    /// `Some(when)`: a failed lookup's stand-in answer, superseded once `when` has passed.
    retry_after: Option<f64>,
}

struct LookupBatch {
    pointers: Vec<String>,
    task: ActiveEntityTask,
}

/// State of the [`ForeignEmoteRelayPlugin`]: per-player report queues, the pointer → `loop` cache
/// and the metadata lookups in flight.
#[derive(Resource, Default)]
pub struct ForeignEmoteRelay {
    pending: HashMap<Entity, VecDeque<PendingReport>>,
    /// urn and loop of the last start reported per player, so a stop can echo them
    started: HashMap<Entity, (String, bool)>,
    cache: HashMap<String, CachedLoop>,
    /// insertion order of `cache`, for eviction
    cache_order: VecDeque<String>,
    /// pointers with a lookup in flight, or waiting for a batch slot
    requested: HashSet<String>,
    /// pointers waiting for a request slot, oldest first
    wanted: VecDeque<String>,
    batches: Vec<LookupBatch>,
    sequence: u32,
}

impl ForeignEmoteRelay {
    fn remember(&mut self, pointer: String, loops: bool, retry_after: Option<f64>) {
        if self
            .cache
            .insert(pointer.clone(), CachedLoop { loops, retry_after })
            .is_none()
        {
            self.cache_order.push_back(pointer);
            while self.cache_order.len() > MAX_CACHED_POINTERS {
                if let Some(oldest) = self.cache_order.pop_front() {
                    self.cache.remove(&oldest);
                }
            }
        }
    }
}

/// The cached `loop` for a pointer, unless it is a failed lookup's stand-in whose retry is due.
fn cached_loop(cache: &HashMap<String, CachedLoop>, pointer: &str, now: f64) -> Option<bool> {
    cache
        .get(pointer)
        .filter(|cached| cached.retry_after.is_none_or(|when| now < when))
        .map(|cached| cached.loops)
}

/// Queue each wire event behind the player's earlier ones.
fn queue_foreign_emote_reports(
    mut events: EventReader<ForeignEmoteEvent>,
    players: Query<&ForeignPlayer>,
    mut relay: ResMut<ForeignEmoteRelay>,
) {
    for event in events.read() {
        // gone before we got here: nothing to report it against
        let Ok(player) = players.get(event.player) else {
            continue;
        };
        let report = match &event.kind {
            ForeignEmoteEventKind::Started { urn, mask } => {
                if !foreign_emote_urn_is_acceptable(urn) {
                    debug!(
                        "dropping emote with unacceptable urn from {:#x}",
                        player.address
                    );
                    continue;
                }
                // an identifier no catalyst could resolve still names a playback: report it as a
                // one-shot rather than losing the event
                let source = classify_emote(urn).unwrap_or(LoopSource::Known(false));
                PendingReport::Start {
                    urn: urn.clone(),
                    mask: mask.filter(|mask| valid_avatar_mask(*mask)),
                    source,
                }
            }
            ForeignEmoteEventKind::Stopped { completed } => PendingReport::Stop {
                completed: *completed,
            },
        };
        let queue = relay.pending.entry(event.player).or_default();
        if queue.len() >= MAX_PENDING_PER_PLAYER {
            // full. Only a start awaiting its loop can hold a queue: report it as a one-shot so
            // the queue drains, rather than drop anything and part a start from its stop
            if let Some(PendingReport::Start { source, .. }) = queue.front_mut() {
                if matches!(source, LoopSource::Pointer(_)) {
                    debug!(
                        "emote queue full for {:#x}; reporting its blocked start as a one-shot",
                        player.address
                    );
                    *source = LoopSource::Known(false);
                }
            }
        }
        queue.push_back(report);
        // last resort against a single-frame burst from one peer
        while queue.len() > 2 * MAX_PENDING_PER_PLAYER {
            queue.pop_front();
        }
    }
}

/// What to do with the head of a player's queue.
enum HeadAction {
    Write {
        urn: String,
        loops: bool,
        mask: Option<i32>,
        state: EmoteState,
    },
    Skip,
    Wait,
}

/// Drain each player's queue for as long as its head can be written.
fn flush_foreign_emote_reports(
    players: Query<&ForeignPlayer>,
    mut contexts: Query<&mut GlobalCrdtState>,
    mut relay: ResMut<ForeignEmoteRelay>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs_f64();
    let ForeignEmoteRelay {
        pending,
        started,
        cache,
        requested,
        wanted,
        sequence,
        ..
    } = &mut *relay;

    pending.retain(|player, queue| {
        let Ok(foreign) = players.get(*player) else {
            started.remove(player);
            return false;
        };
        let Ok(mut state) = contexts.get_mut(foreign.context) else {
            return false;
        };

        while let Some(head) = queue.front() {
            let action = match head {
                PendingReport::Start { urn, mask, source } => {
                    let loops = match source {
                        LoopSource::Known(loops) => Some(*loops),
                        LoopSource::Pointer(pointer) => match cached_loop(cache, pointer, now) {
                            Some(loops) => Some(loops),
                            // asked already; the answer is on its way
                            None if requested.contains(pointer) => None,
                            None if wanted.len() >= MAX_WANTED_POINTERS => {
                                debug!("emote lookup queue full; reporting {pointer} as a one-shot");
                                Some(false)
                            }
                            None => {
                                requested.insert(pointer.clone());
                                wanted.push_back(pointer.clone());
                                None
                            }
                        },
                    };
                    match loops {
                        Some(loops) => HeadAction::Write {
                            urn: urn.clone(),
                            loops,
                            mask: *mask,
                            state: EmoteState::EsStarted,
                        },
                        None => HeadAction::Wait,
                    }
                }
                PendingReport::Stop { completed } => match started.remove(player) {
                    Some((urn, loops)) => HeadAction::Write {
                        urn,
                        loops,
                        mask: None,
                        state: if *completed {
                            EmoteState::EsFinished
                        } else {
                            EmoteState::EsInterrupted
                        },
                    },
                    // a stop with no start on record
                    None => HeadAction::Skip,
                },
            };

            match action {
                HeadAction::Wait => break,
                HeadAction::Skip => {
                    queue.pop_front();
                }
                HeadAction::Write {
                    urn,
                    loops,
                    mask,
                    state: emote_state,
                } => {
                    if emote_state == EmoteState::EsStarted {
                        started.insert(*player, (urn.clone(), loops));
                    }
                    *sequence = sequence.wrapping_add(1);
                    state.update_crdt(
                        SceneComponentId::AVATAR_EMOTE_COMMAND,
                        CrdtType::GO_ANY,
                        foreign.scene_id,
                        &PbAvatarEmoteCommand {
                            emote_urn: urn,
                            r#loop: loops,
                            timestamp: *sequence,
                            mask,
                            state: Some(emote_state as i32),
                        },
                    );
                    queue.pop_front();
                }
            }
        }
        !queue.is_empty()
    });
}

/// The next request's pointers: up to `MAX_LOOKUP_POINTERS_PER_REQUEST`, oldest first.
fn take_lookup_chunk(wanted: &mut VecDeque<String>) -> Vec<String> {
    let count = wanted.len().min(MAX_LOOKUP_POINTERS_PER_REQUEST);
    wanted.drain(..count).collect()
}

/// Bank a landed lookup's answers for the `pointers` it asked about. Returned pointers go through
/// the same canonicalisation as requested ones so an answer always finds its question; a pointer the
/// catalyst did not answer is a definitive "no such emote" and is cached as a one-shot too, else an
/// unknown urn would be looked up again on every trigger. A failed request answers `false` for
/// `LOOKUP_RETRY_SECS` and is then retried.
fn bank_lookup(
    relay: &mut ForeignEmoteRelay,
    pointers: Vec<String>,
    result: Result<Vec<EntityDefinition>, anyhow::Error>,
    now: f64,
) {
    match result {
        Ok(entities) => {
            let mut answered: HashSet<String> = HashSet::default();
            for entity in entities {
                let loops = entity
                    .metadata
                    .as_ref()
                    .and_then(loop_from_emote_metadata)
                    .unwrap_or(false);
                for pointer in entity.pointers {
                    let pointer = canonical_pointer(&pointer);
                    answered.insert(pointer.clone());
                    relay.remember(pointer, loops, None);
                }
            }
            for pointer in &pointers {
                if !answered.contains(pointer) {
                    relay.remember(pointer.clone(), false, None);
                }
            }
        }
        Err(e) => {
            warn!("emote metadata lookup failed: {e}");
            for pointer in &pointers {
                relay.remember(pointer.clone(), false, Some(now + LOOKUP_RETRY_SECS));
            }
        }
    }
    for pointer in &pointers {
        relay.requested.remove(pointer);
    }
}

/// Bank the answers of landed metadata lookups and start requests for the pointers now wanted.
fn resolve_foreign_emote_metadata(
    mut relay: ResMut<ForeignEmoteRelay>,
    ipfas: IpfsAssetServer,
    time: Res<Time>,
) {
    let now = time.elapsed_secs_f64();

    let mut landed = Vec::new();
    relay
        .batches
        .retain_mut(|batch| match batch.task.complete() {
            Some(result) => {
                landed.push((std::mem::take(&mut batch.pointers), result));
                false
            }
            None => true,
        });
    for (pointers, result) in landed {
        bank_lookup(&mut relay, pointers, result, now);
    }

    while !relay.wanted.is_empty() && relay.batches.len() < MAX_LOOKUP_BATCHES {
        let pointers = take_lookup_chunk(&mut relay.wanted);
        let task = ipfas.ipfs().active_entities(
            ActiveEntitiesRequest::Pointers(pointers.clone()),
            Some(&emote_catalyst_url()),
        );
        relay.batches.push(LookupBatch { pointers, task });
    }
}

#[cfg(test)]
mod foreign_emote_relay_tests {
    use super::*;
    use dcl::interface::CrdtMessageType;
    use prost::Message as _;

    const SCENE_LOOPING: &str = "urn:decentraland:off-chain:scene-emote:bafyhash-true";
    const WEARABLE: &str = "urn:decentraland:matic:collections-v2:0xAbC:0:12345";
    const WEARABLE_POINTER: &str = "urn:decentraland:matic:collections-v2:0xabc:0";

    fn app() -> App {
        let mut app = App::new();
        app.init_resource::<Time>();
        app.init_resource::<ForeignEmoteRelay>();
        app.add_event::<ForeignEmoteEvent>();
        // the lookup system needs a realm; these tests bank answers into the cache directly
        app.add_systems(
            Update,
            (queue_foreign_emote_reports, flush_foreign_emote_reports).chain(),
        );
        app
    }

    fn spawn_context(app: &mut App) -> (Entity, broadcast::Receiver<GlobalCrdtStateUpdate>) {
        let state = GlobalCrdtState::new(None);
        let (_, receiver) = state.subscribe(Vec3::ZERO);
        (app.world_mut().spawn(state).id(), receiver)
    }

    fn spawn_player(app: &mut App, context: Entity, scene_id: SceneEntityId) -> Entity {
        let (audio_sender, _audio_receiver) = mpsc::channel::<ForeignAudioData>(1);
        app.world_mut()
            .spawn(ForeignPlayer {
                address: Address::from_low_u64_be(0x1234),
                context,
                transports: HashSet::default(),
                scene_id,
                profile_version: 0,
                audio_sender,
            })
            .id()
    }

    fn started(app: &mut App, player: Entity, urn: &str) {
        app.world_mut().send_event(ForeignEmoteEvent {
            player,
            kind: ForeignEmoteEventKind::Started {
                urn: urn.to_owned(),
                mask: None,
            },
        });
    }

    fn stopped(app: &mut App, player: Entity, completed: bool) {
        app.world_mut().send_event(ForeignEmoteEvent {
            player,
            kind: ForeignEmoteEventKind::Stopped { completed },
        });
    }

    fn resolve(app: &mut App, pointer: &str, loops: bool) {
        app.world_mut()
            .resource_mut::<ForeignEmoteRelay>()
            .remember(pointer.to_owned(), loops, None);
    }

    /// Every `AvatarEmoteCommand` appended for `scene_id` in the context's store, oldest first.
    fn reported(app: &App, context: Entity, scene_id: SceneEntityId) -> Vec<PbAvatarEmoteCommand> {
        app.world()
            .get::<GlobalCrdtState>(context)
            .unwrap()
            .store
            .go
            .get(&SceneComponentId::AVATAR_EMOTE_COMMAND)
            .and_then(|state| state.0.get(&scene_id))
            .map(|entries| {
                entries
                    .iter()
                    .map(|entry| PbAvatarEmoteCommand::decode(entry.data.as_slice()).unwrap())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn started_with_mask(app: &mut App, player: Entity, urn: &str, mask: i32) {
        app.world_mut().send_event(ForeignEmoteEvent {
            player,
            kind: ForeignEmoteEventKind::Started {
                urn: urn.to_owned(),
                mask: Some(mask),
            },
        });
    }

    fn looping_entity(pointer: &str) -> EntityDefinition {
        EntityDefinition {
            id: "bafyentity".to_owned(),
            pointers: vec![pointer.to_owned()],
            content: Default::default(),
            metadata: Some(serde_json::json!({ "emoteDataADR74": { "loop": true } })),
        }
    }

    fn state_of(command: &PbAvatarEmoteCommand) -> EmoteState {
        match command.state.expect("state is always written") {
            s if s == EmoteState::EsStarted as i32 => EmoteState::EsStarted,
            s if s == EmoteState::EsFinished as i32 => EmoteState::EsFinished,
            s if s == EmoteState::EsInterrupted as i32 => EmoteState::EsInterrupted,
            other => panic!("unknown emote state {other}"),
        }
    }

    /// The entities named by the APPEND_VALUE messages broadcast to subscribed scenes so far.
    fn broadcast_appends(
        receiver: &mut broadcast::Receiver<GlobalCrdtStateUpdate>,
    ) -> Vec<SceneEntityId> {
        let mut entities = Vec::new();
        while let Ok(update) = receiver.try_recv() {
            let GlobalCrdtStateUpdate::Crdt(bytes, localizer) = update else {
                continue;
            };
            assert!(matches!(localizer, Localizer::None));
            let mut reader = DclReader::new(&bytes);
            let _length = reader.read_u32().unwrap();
            assert_eq!(
                reader.read_u32().unwrap(),
                CrdtMessageType::AppendValue as u32
            );
            let entity: SceneEntityId = reader.read().unwrap();
            let component: SceneComponentId = reader.read().unwrap();
            assert_eq!(component, SceneComponentId::AVATAR_EMOTE_COMMAND);
            entities.push(entity);
        }
        entities
    }

    #[test]
    fn reports_start_then_finish_with_monotonic_timestamps() {
        let mut app = app();
        let (context, mut receiver) = spawn_context(&mut app);
        let scene_id = SceneEntityId::new(32, 0);
        let player = spawn_player(&mut app, context, scene_id);

        started(&mut app, player, SCENE_LOOPING);
        app.update();
        // nothing new: no re-report
        app.update();
        stopped(&mut app, player, true);
        app.update();

        let commands = reported(&app, context, scene_id);
        assert_eq!(commands.len(), 2);
        assert_eq!(state_of(&commands[0]), EmoteState::EsStarted);
        assert_eq!(state_of(&commands[1]), EmoteState::EsFinished);
        for command in &commands {
            assert_eq!(command.emote_urn, SCENE_LOOPING);
            assert!(command.r#loop);
        }
        assert!(commands[0].timestamp < commands[1].timestamp);
        assert_eq!(broadcast_appends(&mut receiver), vec![scene_id, scene_id]);
    }

    #[test]
    fn waits_for_metadata_and_keeps_wire_order() {
        let mut app = app();
        let (context, _receiver) = spawn_context(&mut app);
        let scene_id = SceneEntityId::new(33, 0);
        let player = spawn_player(&mut app, context, scene_id);

        started(&mut app, player, WEARABLE);
        app.update();
        assert!(reported(&app, context, scene_id).is_empty());
        {
            let relay = app.world().resource::<ForeignEmoteRelay>();
            assert!(relay.requested.contains(WEARABLE_POINTER));
            assert_eq!(relay.wanted, VecDeque::from([WEARABLE_POINTER.to_owned()]));
        }

        // the cancel lands before the metadata does; it must not overtake the start
        stopped(&mut app, player, false);
        app.update();
        assert!(reported(&app, context, scene_id).is_empty());

        resolve(&mut app, WEARABLE_POINTER, true);
        app.update();

        let commands = reported(&app, context, scene_id);
        assert_eq!(commands.len(), 2);
        assert_eq!(state_of(&commands[0]), EmoteState::EsStarted);
        assert_eq!(state_of(&commands[1]), EmoteState::EsInterrupted);
        for command in &commands {
            // the full instance urn is reported, not the pointer it resolved through
            assert_eq!(command.emote_urn, WEARABLE);
            assert!(command.r#loop);
        }
        // the pointer is asked for once
        assert_eq!(
            app.world().resource::<ForeignEmoteRelay>().wanted,
            VecDeque::from([WEARABLE_POINTER.to_owned()])
        );
    }

    #[test]
    fn unresolvable_identifier_is_reported_as_a_one_shot() {
        let mut app = app();
        let (context, _receiver) = spawn_context(&mut app);
        let scene_id = SceneEntityId::new(34, 0);
        let player = spawn_player(&mut app, context, scene_id);

        started(
            &mut app,
            player,
            "urn:decentraland:off-chain:no-such-collection:x",
        );
        app.update();

        let commands = reported(&app, context, scene_id);
        assert_eq!(commands.len(), 1);
        assert!(!commands[0].r#loop);
        assert_eq!(state_of(&commands[0]), EmoteState::EsStarted);
    }

    #[test]
    fn drops_unacceptable_urns_and_orphan_stops() {
        let mut app = app();
        let (context, mut receiver) = spawn_context(&mut app);
        let scene_id = SceneEntityId::new(35, 0);
        let player = spawn_player(&mut app, context, scene_id);

        for urn in [
            String::new(),
            "x".repeat(MAX_EMOTE_URN_BYTES + 1),
            "urn:decentraland:off-chain:base-emotes:wave dance".to_owned(),
            "urn:decentraland:off-chain:base-emotes:wave\u{1b}[31m".to_owned(),
        ] {
            started(&mut app, player, &urn);
            app.update();
        }
        // no start on record
        stopped(&mut app, player, true);
        app.update();

        assert!(reported(&app, context, scene_id).is_empty());
        assert!(broadcast_appends(&mut receiver).is_empty());
    }

    #[test]
    fn writes_only_to_the_players_own_context() {
        let mut app = app();
        let (shared, mut shared_receiver) = spawn_context(&mut app);
        let (room, mut room_receiver) = spawn_context(&mut app);
        let scene_id = SceneEntityId::new(36, 0);
        let player = spawn_player(&mut app, room, scene_id);

        started(&mut app, player, SCENE_LOOPING);
        app.update();

        assert!(reported(&app, shared, scene_id).is_empty());
        assert_eq!(reported(&app, room, scene_id).len(), 1);
        assert!(broadcast_appends(&mut shared_receiver).is_empty());
        assert_eq!(broadcast_appends(&mut room_receiver), vec![scene_id]);
    }

    #[test]
    fn classifies_emote_identifiers() {
        assert_eq!(
            classify_emote("urn:decentraland:off-chain:scene-emote:bafyhash-true"),
            Some(LoopSource::Known(true))
        );
        assert_eq!(
            classify_emote("urn:decentraland:off-chain:scene-emote:bafyhash-false"),
            Some(LoopSource::Known(false))
        );
        assert_eq!(
            classify_emote("sittingChair1"),
            Some(LoopSource::Known(true))
        );
        assert_eq!(classify_emote("Waving"), Some(LoopSource::Known(false)));
        assert_eq!(
            classify_emote("Wave"),
            Some(LoopSource::Pointer(
                "urn:decentraland:off-chain:base-emotes:wave".to_owned()
            ))
        );
        assert_eq!(
            classify_emote("urn:decentraland:off-chain:base-emotes:Dance"),
            Some(LoopSource::Pointer(
                "urn:decentraland:off-chain:base-emotes:dance".to_owned()
            ))
        );
        assert_eq!(
            classify_emote(WEARABLE),
            Some(LoopSource::Pointer(WEARABLE_POINTER.to_owned()))
        );
        assert_eq!(
            classify_emote("urn:decentraland:matic:collections-thirdparty:tp:coll:item:1:2:3"),
            Some(LoopSource::Pointer(
                "urn:decentraland:matic:collections-thirdparty:tp:coll:item".to_owned()
            ))
        );
        assert_eq!(
            classify_emote("urn:decentraland:matic:collections-v2:0xabc"),
            None
        );
        assert_eq!(
            classify_emote("urn:decentraland:off-chain:no-such-collection:x"),
            None
        );
    }

    #[test]
    fn reads_loop_from_deployed_metadata() {
        let adr74 = serde_json::json!({ "emoteDataADR74": { "loop": true } });
        let legacy = serde_json::json!({ "data": { "loop": false } });
        let neither = serde_json::json!({ "name": "wave" });
        assert_eq!(loop_from_emote_metadata(&adr74), Some(true));
        assert_eq!(loop_from_emote_metadata(&legacy), Some(false));
        assert_eq!(loop_from_emote_metadata(&neither), None);
    }

    #[test]
    fn failed_lookup_answers_false_until_its_retry_is_due() {
        let mut relay = ForeignEmoteRelay::default();
        relay.remember(WEARABLE_POINTER.to_owned(), false, Some(10.0));
        assert_eq!(
            cached_loop(&relay.cache, WEARABLE_POINTER, 0.0),
            Some(false)
        );
        assert_eq!(cached_loop(&relay.cache, WEARABLE_POINTER, 10.0), None);
        relay.remember(WEARABLE_POINTER.to_owned(), true, None);
        assert_eq!(
            cached_loop(&relay.cache, WEARABLE_POINTER, 10.0),
            Some(true)
        );
    }

    #[test]
    fn evicts_the_oldest_cached_pointers() {
        let mut relay = ForeignEmoteRelay::default();
        for i in 0..=MAX_CACHED_POINTERS {
            relay.remember(format!("pointer-{i}"), true, None);
        }
        assert_eq!(relay.cache.len(), MAX_CACHED_POINTERS);
        assert!(!relay.cache.contains_key("pointer-0"));
        assert!(relay
            .cache
            .contains_key(&format!("pointer-{MAX_CACHED_POINTERS}")));
    }

    #[test]
    fn canonicalises_b64_segments_on_both_paths() {
        // a preview-server item id: base64 is case-sensitive, so `b64-` segments keep their case
        // while the rest of the urn is lowercased — on the request AND the answer
        const URN: &str = "urn:decentraland:matic:collections-v2:b64-QWJjRGVm:0:7";
        const POINTER: &str = "urn:decentraland:matic:collections-v2:b64-QWJjRGVm:0";
        assert_eq!(
            classify_emote(URN),
            Some(LoopSource::Pointer(POINTER.to_owned()))
        );

        let mut app = app();
        let (context, _receiver) = spawn_context(&mut app);
        let scene_id = SceneEntityId::new(40, 0);
        let player = spawn_player(&mut app, context, scene_id);
        started(&mut app, player, URN);
        app.update();
        assert!(reported(&app, context, scene_id).is_empty());

        // the catalyst echoes the pointer in its own casing
        let answered = "URN:DECENTRALAND:MATIC:COLLECTIONS-V2:b64-QWJjRGVm:0";
        {
            let mut relay = app.world_mut().resource_mut::<ForeignEmoteRelay>();
            bank_lookup(
                &mut relay,
                vec![POINTER.to_owned()],
                Ok(vec![looping_entity(answered)]),
                0.0,
            );
            assert_eq!(relay.cache.len(), 1, "one key for request and answer");
            assert!(relay.cache.contains_key(POINTER));
            assert!(!relay.requested.contains(POINTER));
        }
        app.update();

        let commands = reported(&app, context, scene_id);
        assert_eq!(commands.len(), 1);
        assert!(commands[0].r#loop, "the looping answer reached the report");
        assert_eq!(commands[0].emote_urn, URN);
    }

    #[test]
    fn unanswered_and_failed_lookups_are_banked_distinctly() {
        let mut relay = ForeignEmoteRelay::default();
        relay.requested.insert("a".to_owned());
        relay.requested.insert("b".to_owned());
        // `a` answered, `b` not: `b` is a definitive miss, cached for good
        bank_lookup(
            &mut relay,
            vec!["a".to_owned(), "b".to_owned()],
            Ok(vec![looping_entity("a")]),
            0.0,
        );
        assert_eq!(cached_loop(&relay.cache, "a", 1e9), Some(true));
        assert_eq!(cached_loop(&relay.cache, "b", 1e9), Some(false));
        assert!(relay.requested.is_empty());

        // a failed request is a stand-in answer that expires
        relay.requested.insert("c".to_owned());
        bank_lookup(
            &mut relay,
            vec!["c".to_owned()],
            Err(anyhow::anyhow!("offline")),
            5.0,
        );
        assert_eq!(cached_loop(&relay.cache, "c", 5.0), Some(false));
        assert_eq!(
            cached_loop(&relay.cache, "c", 5.0 + LOOKUP_RETRY_SECS),
            None
        );
        assert!(relay.requested.is_empty());
    }

    #[test]
    fn caps_the_lookup_queue_and_chunks_requests() {
        let mut app = app();
        let (context, _receiver) = spawn_context(&mut app);
        // more players than the wait queue holds, each triggering a distinct unresolved emote
        let players: Vec<(Entity, SceneEntityId)> = (0..MAX_WANTED_POINTERS + 44)
            .map(|i| {
                let scene_id = SceneEntityId::new(1000 + i as u16, 0);
                (spawn_player(&mut app, context, scene_id), scene_id)
            })
            .collect();
        for (i, (player, _)) in players.iter().enumerate() {
            started(
                &mut app,
                *player,
                &format!("urn:decentraland:matic:collections-v2:0xabc:{i}:1"),
            );
        }
        app.update();

        {
            let relay = app.world().resource::<ForeignEmoteRelay>();
            assert_eq!(relay.wanted.len(), MAX_WANTED_POINTERS);
            assert_eq!(relay.requested.len(), MAX_WANTED_POINTERS);
        }
        // the overflow was reported at once as one-shots, not queued or dropped
        let overflow: Vec<PbAvatarEmoteCommand> = players
            .iter()
            .flat_map(|(_, scene_id)| reported(&app, context, *scene_id))
            .collect();
        assert_eq!(overflow.len(), 44);
        assert!(overflow.iter().all(|command| !command.r#loop));
        {
            let relay = app.world().resource::<ForeignEmoteRelay>();
            assert_eq!(relay.cache.len(), 0, "overflow answers are not cached");
        }

        // requests go out in fixed-size chunks, oldest first
        let mut wanted =
            std::mem::take(&mut app.world_mut().resource_mut::<ForeignEmoteRelay>().wanted);
        // players flush in map order, so which pointer is oldest is not the spawn order; what
        // matters is that a chunk takes from the front
        let oldest = wanted.front().cloned().unwrap();
        let first = take_lookup_chunk(&mut wanted);
        assert_eq!(first.len(), MAX_LOOKUP_POINTERS_PER_REQUEST);
        assert_eq!(first[0], oldest);
        assert_eq!(
            wanted.len(),
            MAX_WANTED_POINTERS - MAX_LOOKUP_POINTERS_PER_REQUEST
        );
        let mut chunks = 1;
        while !wanted.is_empty() {
            assert!(take_lookup_chunk(&mut wanted).len() <= MAX_LOOKUP_POINTERS_PER_REQUEST);
            chunks += 1;
        }
        assert_eq!(
            chunks,
            MAX_WANTED_POINTERS / MAX_LOOKUP_POINTERS_PER_REQUEST
        );
    }

    #[test]
    fn full_queue_force_resolves_its_blocked_head() {
        let mut app = app();
        let (context, _receiver) = spawn_context(&mut app);
        let scene_id = SceneEntityId::new(41, 0);
        let player = spawn_player(&mut app, context, scene_id);

        // a start awaiting metadata, then a backlog behind it
        started(&mut app, player, WEARABLE);
        for _ in 0..MAX_PENDING_PER_PLAYER {
            stopped(&mut app, player, false);
        }
        app.update();

        let commands = reported(&app, context, scene_id);
        assert_eq!(
            commands.len(),
            2,
            "the start went out (as a one-shot) and its stop followed"
        );
        assert_eq!(state_of(&commands[0]), EmoteState::EsStarted);
        assert!(!commands[0].r#loop);
        assert_eq!(state_of(&commands[1]), EmoteState::EsInterrupted);
        assert_eq!(commands[1].emote_urn, WEARABLE);
        let relay = app.world().resource::<ForeignEmoteRelay>();
        assert!(relay.pending.is_empty(), "the backlog drained");
        assert!(
            !relay.cache.contains_key(WEARABLE_POINTER),
            "a forced answer is not cached"
        );
    }

    #[test]
    fn writes_only_masks_the_enum_defines() {
        let mut app = app();
        let (context, _receiver) = spawn_context(&mut app);
        let scene_id = SceneEntityId::new(42, 0);
        let player = spawn_player(&mut app, context, scene_id);

        started_with_mask(
            &mut app,
            player,
            SCENE_LOOPING,
            AvatarMask::AmUpperBody as i32,
        );
        app.update();
        started_with_mask(&mut app, player, SCENE_LOOPING, 7);
        app.update();

        let commands = reported(&app, context, scene_id);
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].mask, Some(AvatarMask::AmUpperBody as i32));
        assert_eq!(commands[1].mask, None);
    }
}
