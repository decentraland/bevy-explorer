use std::{f32::consts::TAU, ops::RangeInclusive, sync::Arc};

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
};
use ethers_core::types::Address;
use serde::{Deserialize, Serialize};
use serde_json::json;
use system_bridge::{SystemApi, VoiceMessage};
use tokio::sync::{broadcast, mpsc, oneshot};

use dcl::{
    crdt::{append_component, delete_entity, put_component},
    interface::{crdt_context::CrdtContext, CrdtStore, CrdtType},
    js::comms::CommsMessageType,
    SceneId,
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

const FOREIGN_PLAYER_RANGE: RangeInclusive<u16> = 6..=406;

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
        let (ext_sender, ext_receiver) = mpsc::channel(1000);
        let (int_sender, _) = broadcast::channel(1000);
        app.init_resource::<DiscardPlayerUpdates>();
        app.insert_resource(GlobalCrdtState {
            ext_receiver,
            ext_sender,
            int_sender,
            context: CrdtContext::new(
                SceneId::DUMMY,
                "Global Crdt".into(),
                "Global Crdt".into(),
                false,
                false,
            ),
            store: Default::default(),
            lookup: Default::default(),
            realm_bounds: (IVec2::MAX, IVec2::MIN),
            localizers: Default::default(),
        });

        let (sender, _) = tokio::sync::broadcast::channel(1_000);
        app.insert_resource(LocalAudioSource { sender });

        app.init_resource::<VoiceMessageStreams>();

        app.add_systems(Update, process_transport_updates);
        app.add_systems(Update, despawn_players);
        app.add_observer(remove_transport_from_foreign_audio_source);
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
    MetaData(String),
    PlayerData(rfc4::packet::Message),
    /// Pulse-decoded movement, delivered natively rather than as an rfc4 `Movement` packet (those
    /// are no longer supported). Boxed because the decoder already hands us a `Box`.
    Movement(Box<rfc4::Movement>),
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
            Self::MetaData(arg0) => f.debug_tuple("MetaData").field(arg0).finish(),
            Self::PlayerData(arg0) => f.debug_tuple("PlayerData").field(arg0).finish(),
            Self::Movement(arg0) => f.debug_tuple("Movement").field(arg0).finish(),
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

#[derive(Resource)]
pub struct GlobalCrdtState {
    // receiver from sockets
    ext_receiver: mpsc::Receiver<NetworkUpdate>,
    // sender for sockets to post to
    ext_sender: mpsc::Sender<NetworkUpdate>,
    // sender for broadcast updates
    int_sender: broadcast::Sender<GlobalCrdtStateUpdate>,
    context: CrdtContext,
    store: CrdtStore,
    lookup: BiMap<Address, Entity>,
    pub(crate) realm_bounds: (IVec2, IVec2),
    // per-component localizer registry (populated as components are first sent)
    localizers: HashMap<SceneComponentId, Localizer>,
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
        if let Err(e) = self
            .int_sender
            .send(GlobalCrdtStateUpdate::Crdt(crdt_message, localizer))
        {
            error!("failed to send foreign player update to scenes: {e}");
        }
    }

    pub fn delete_entity(&mut self, id: SceneEntityId) {
        self.store.clean_up(&HashSet::from_iter(Some(id)));
        let crdt_message = delete_entity(&id);
        if let Err(e) = self
            .int_sender
            .send(GlobalCrdtStateUpdate::Crdt(crdt_message, Localizer::None))
        {
            error!("failed to send foreign player update to scenes: {e}");
        }
    }

    pub fn update_time(&mut self, time: f32) {
        if let Err(e) = self.int_sender.send(GlobalCrdtStateUpdate::Time(time)) {
            error!("failed to send time update to scenes: {e}");
        }
    }

    pub fn update_camera_fov(&mut self, fov_y: f32) {
        if let Err(e) = self
            .int_sender
            .send(GlobalCrdtStateUpdate::CameraFov(fov_y))
        {
            error!("failed to send camera fov update to scenes: {e}");
        }
    }
}

#[derive(Component, Debug)]
pub struct ForeignPlayer {
    pub address: Address,
    pub transport_id: Entity,
    pub last_update: f32,
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
    /// Server tick (seconds) — orders updates and anchors the shared clock SDA aligns against.
    pub timestamp: f32,
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
    Request(rfc4::ProfileRequest),
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
fn apply_foreign_movement(
    m: &rfc4::Movement,
    entity: Entity,
    scene_id: SceneEntityId,
    now: f32,
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
    position_events.write(PlayerPositionEvent {
        player: entity,
        time: now,
        timestamp: m.timestamp,
        translation: dcl_transform.translation,
        rotation: dcl_transform.rotation,
        velocity: vel,
        grounded: Some(m.is_grounded),
        remote_move_kind,
    });
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn process_transport_updates(
    mut commands: Commands,
    mut state: ResMut<GlobalCrdtState>,
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
                let (entity, scene_id, audio_channel) = if let Some((entity, scene_id, channel)) =
                    created_this_frame.get(&update.address)
                {
                    (*entity, *scene_id, channel.clone())
                } else if let Some(existing) = state.lookup.get_by_left(&update.address) {
                    let mut foreign_player = players.get_mut(*existing).unwrap();
                    foreign_player.last_update = time.elapsed_secs();
                    foreign_player.transport_id = update.transport_id;
                    (
                        *existing,
                        foreign_player.scene_id,
                        foreign_player.audio_sender.clone(),
                    )
                } else {
                    let Some(next_free) = state.context.new_in_range(&FOREIGN_PLAYER_RANGE) else {
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

                    let (audio_sender, audio_receiver) = mpsc::channel::<ForeignAudioData>(10);

                    let new_entity = commands
                        .spawn((
                            Transform::default(),
                            Visibility::default(),
                            ForeignPlayer {
                                address: update.address,
                                transport_id: update.transport_id,
                                last_update: time.elapsed_secs(),
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
                        let _ =
                            audio_channel.try_send(ForeignAudioData::TransportAvailable(transport));
                    }
                    PlayerMessage::AudioStreamUnavailable { transport } => {
                        // pass through
                        debug!("{transport} not available for {entity}!");
                        let _ = audio_channel
                            .try_send(ForeignAudioData::TransportUnavailable(transport));
                    }
                    // Legacy rfc4 index-ordered Position and bit-packed MovementCompressed are no
                    // longer supported. Uncompressed Movement IS still ingested (next arm) for peers
                    // on a non-Pulse / local-websocket realm; on a Pulse realm the same state arrives
                    // as `PlayerMessage::Movement` below.
                    PlayerMessage::PlayerData(
                        Message::Position(_) | Message::MovementCompressed(_),
                    ) => {}
                    PlayerMessage::PlayerData(Message::Movement(m)) => apply_foreign_movement(
                        &m,
                        entity,
                        scene_id,
                        time.elapsed_secs(),
                        &mut commands,
                        &mut state,
                        &mut position_events,
                    ),
                    PlayerMessage::PlayerData(Message::ProfileVersion(version)) => {
                        profile_events.write(ProfileEvent {
                            sender: entity,
                            event: ProfileEventType::Version(version),
                        });
                    }
                    PlayerMessage::PlayerData(Message::ProfileRequest(request)) => {
                        profile_events.write(ProfileEvent {
                            sender: entity,
                            event: ProfileEventType::Request(request),
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
                        process_messagebus(
                            scene,
                            address,
                            &mut string_senders,
                            &mut binary_senders,
                        );
                    }
                    PlayerMessage::PlayerData(Message::Voice(_)) => (),
                    PlayerMessage::Movement(m) => apply_foreign_movement(
                        &m,
                        entity,
                        scene_id,
                        time.elapsed_secs(),
                        &mut commands,
                        &mut state,
                        &mut position_events,
                    ),
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
                    PlayerMessage::PlayerData(Message::PlayerEmote(emote)) => {
                        debug!("emote: {emote:?}");
                        commands.entity(entity).try_insert(EmoteCommand {
                            timestamp: emote.incremental_id as i64,
                            urn: emote.urn,
                            r#loop: false,
                        });
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

                process_messagebus(
                    scene,
                    update.address,
                    &mut string_senders,
                    &mut binary_senders,
                );
            }
        }
    }
}

// Resolves a standalone `SceneDrivenAnimation` packet into the full state. An empty
// `scene_hash` clears the state; absent hash fields (None) reuse the last cached pair
// for this entity so we keep animating between keepalives. Returns `None` when the
// sender has no active scene-driven animation. The resolved state is emitted as a
// `PlayerSceneAnimEvent` so `foreign_dynamics` can apply it after the foreign-position
// interpolation lag, lined up with the visible avatar.
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
    mut state: ResMut<GlobalCrdtState>,
    time: Res<Time>,
) {
    for (entity, player) in players.iter() {
        if player.last_update + 10.0 < time.elapsed_secs() {
            if let Ok(mut commands) = commands.get_entity(entity) {
                info!("removing stale player: {entity:?} : {player:?}");
                commands.despawn();
            }

            state.delete_entity(player.scene_id);
            state.lookup.remove_by_right(&entity);
        }
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
