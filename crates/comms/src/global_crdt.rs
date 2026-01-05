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
    structs::{AttachPoints, AudioDecoderError, EmoteCommand},
    util::TryPushChildrenEx,
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
    DclReader, DclWriter, SceneComponentId, SceneEntityId, ToDclWriter,
};

#[cfg(all(not(target_arch = "wasm32"), feature = "livekit"))]
use crate::livekit_native::LivekitVideoFrame;
use crate::{
    movement_compressed::MovementCompressed, profile::ProfileMetaCache, SceneRoom, Transport,
};

#[cfg(not(target_arch = "wasm32"))]
use kira::sound::streaming::StreamingSoundData;

#[cfg(target_arch = "wasm32")]
pub struct StreamingSoundData<T>(std::marker::PhantomData<fn() -> T>);

const FOREIGN_PLAYER_RANGE: RangeInclusive<u16> = 6..=406;

pub struct GlobalCrdtPlugin;

impl Plugin for GlobalCrdtPlugin {
    fn build(&self, app: &mut App) {
        let (ext_sender, ext_receiver) = mpsc::channel(1000);
        let (int_sender, _) = broadcast::channel(1000);
        app.insert_resource(GlobalCrdtState {
            ext_receiver,
            ext_sender,
            int_sender,
            context: CrdtContext::new(SceneId::DUMMY, "Global Crdt".into(), false, false),
            store: Default::default(),
            lookup: Default::default(),
            realm_bounds: (IVec2::MAX, IVec2::MIN),
        });

        let (sender, _) = tokio::sync::broadcast::channel(1_000);
        app.insert_resource(LocalAudioSource { sender });

        app.add_systems(Update, process_transport_updates);
        app.add_systems(Update, despawn_players);
        app.add_systems(Update, handle_foreign_audio);
        app.add_systems(Update, pipe_voice_to_scene);
        app.add_event::<PlayerPositionEvent>();
        app.add_event::<ProfileEvent>();
        app.add_event::<ChatEvent>();
    }
}

pub enum PlayerMessage {
    MetaData(String),
    PlayerData(rfc4::packet::Message),
    AudioStreamAvailable { transport: Entity },
    AudioStreamUnavailable { transport: Entity },
}

impl std::fmt::Debug for PlayerMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let var_name = match self {
            Self::MetaData(arg0) => f.debug_tuple("MetaData").field(arg0).finish(),
            Self::PlayerData(arg0) => f.debug_tuple("PlayerData").field(arg0).finish(),
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
    int_sender: broadcast::Sender<Vec<u8>>,
    context: CrdtContext,
    store: CrdtStore,
    lookup: BiMap<Address, Entity>,
    pub(crate) realm_bounds: (IVec2, IVec2),
}

impl GlobalCrdtState {
    // get a channel to which updates can be sent
    pub fn get_sender(&self) -> mpsc::Sender<NetworkUpdate> {
        self.ext_sender.clone()
    }

    // get a channel from which crdt updates can be received
    pub fn subscribe(&self) -> (CrdtStore, broadcast::Receiver<Vec<u8>>) {
        (self.store.clone(), self.int_sender.subscribe())
    }

    pub fn set_bounds(&mut self, min: IVec2, max: IVec2) {
        info!("bounds: {min}-{max}");
        self.realm_bounds = (min, max);
    }
    pub fn update_crdt(
        &mut self,
        component_id: SceneComponentId,
        crdt_type: CrdtType,
        id: SceneEntityId,
        data: &impl ToDclWriter,
    ) {
        let mut buf = Vec::new();
        DclWriter::new(&mut buf).write(data);
        let timestamp =
            self.store
                .force_update(component_id, crdt_type, id, Some(&mut DclReader::new(&buf)));
        let crdt_message = match crdt_type {
            CrdtType::LWW(_) => put_component(&id, &component_id, &timestamp, Some(&buf)),
            CrdtType::GO(_) => append_component(&id, &component_id, &buf),
        };
        if let Err(e) = self.int_sender.send(crdt_message) {
            error!("failed to send foreign player update to scenes: {e}");
        }
    }

    pub fn delete_entity(&mut self, id: SceneEntityId) {
        self.store.clean_up(&HashSet::from_iter(Some(id)));
        let crdt_message = delete_entity(&id);
        if let Err(e) = self.int_sender.send(crdt_message) {
            error!("failed to send foreign player update to scenes: {e}");
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
    #[cfg(all(not(target_arch = "wasm32"), feature = "livekit"))]
    StreamerSubscribe(
        mpsc::Sender<StreamingSoundData<AudioDecoderError>>,
        mpsc::Sender<LivekitVideoFrame>,
    ),
    #[cfg(all(not(target_arch = "wasm32"), feature = "livekit"))]
    StreamerUnsubscribe,
    #[cfg(target_arch = "wasm32")]
    StreamerSubscribe,
    #[cfg(target_arch = "wasm32")]
    StreamerUnsubscribe,
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
#[derive(Event)]
pub struct PlayerPositionEvent {
    pub index: Option<u32>,
    pub player: Entity,
    pub time: f32,
    pub timestamp: Option<f32>,
    pub translation: DclTranslation,
    pub rotation: DclQuat,
    pub velocity: Option<Vec3>,
    pub grounded: Option<bool>,
    pub jumping: Option<bool>,
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

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn process_transport_updates(
    mut commands: Commands,
    mut state: ResMut<GlobalCrdtState>,
    mut players: Query<&mut ForeignPlayer>,
    time: Res<Time>,
    mut profile_events: EventWriter<ProfileEvent>,
    mut position_events: EventWriter<PlayerPositionEvent>,
    mut chat_events: EventWriter<ChatEvent>,
    mut string_senders: Local<HashMap<String, RpcEventSender>>,
    mut binary_senders: Local<HashMap<String, RpcStreamSender<(String, Vec<u8>)>>>,
    mut subscribers: EventReader<RpcCall>,
    mut profile_meta_cache: ResMut<ProfileMetaCache>,
    mut duplicate_chat_filter: Local<HashMap<Entity, f64>>,
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

                    let attach_points = AttachPoints::new(&mut commands);

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
                            Propagate(RenderLayers::default()),
                        ))
                        .try_push_children(&attach_points.entities())
                        .insert(attach_points)
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
                    PlayerMessage::PlayerData(Message::Position(pos)) => {
                        let dcl_transform = DclTransformAndParent {
                            translation: DclTranslation([
                                pos.position_x,
                                pos.position_y,
                                pos.position_z,
                            ]),
                            rotation: DclQuat([
                                pos.rotation_x,
                                pos.rotation_y,
                                pos.rotation_z,
                                pos.rotation_w,
                            ]),
                            scale: Vec3::ONE,
                            parent: SceneEntityId::WORLD_ORIGIN,
                        };
                        debug!(
                            "player: {:#x} -> {}",
                            update.address,
                            Vec3::new(pos.position_x, pos.position_y, pos.position_z)
                        );
                        // commands
                        //     .entity(entity)
                        //     .insert(dcl_transform.to_bevy_transform());
                        state.update_crdt(
                            SceneComponentId::TRANSFORM,
                            CrdtType::LWW_ANY,
                            scene_id,
                            &dcl_transform,
                        );
                        position_events.write(PlayerPositionEvent {
                            index: Some(pos.index),
                            time: time.elapsed_secs(),
                            timestamp: None,
                            player: entity,
                            translation: DclTranslation([
                                pos.position_x,
                                pos.position_y,
                                pos.position_z,
                            ]),
                            rotation: DclQuat([
                                pos.rotation_x,
                                pos.rotation_y,
                                pos.rotation_z,
                                pos.rotation_w,
                            ]),
                            velocity: None,
                            grounded: None,
                            jumping: None,
                        });
                    }
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
                        process_messagebus(
                            scene,
                            format!("{:#x}", update.address),
                            &mut string_senders,
                            &mut binary_senders,
                        );
                    }
                    PlayerMessage::PlayerData(Message::Voice(_)) => (),
                    PlayerMessage::PlayerData(Message::Movement(m)) => {
                        debug!("movement data: {m:?}");
                        let pos = Vec3::new(m.position_x, m.position_y, -m.position_z);
                        let vel = Vec3::new(m.velocity_x, m.velocity_y, -m.velocity_z);
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
                        position_events.write(PlayerPositionEvent {
                            index: None,
                            time: time.elapsed_secs(),
                            timestamp: Some(m.timestamp),
                            player: entity,
                            translation: dcl_transform.translation,
                            rotation: dcl_transform.rotation,
                            velocity: Some(vel),
                            grounded: Some(m.is_grounded),
                            // Some(true) if either is Some(true), else Some(false) if either is Some(false), else None
                            jumping: Some(m.is_jumping || m.is_long_jump),
                        });
                    }
                    PlayerMessage::PlayerData(Message::MovementCompressed(m)) => {
                        debug!("movement compressed data: {m:?}");
                        let movement = MovementCompressed::from_proto(m);
                        let pos = movement.position(state.realm_bounds.0, state.realm_bounds.1);
                        let vel = movement.velocity();
                        let rot = Quat::from_rotation_y(movement.temporal.rotation_f32());
                        let dcl_transform = DclTransformAndParent {
                            translation: DclTranslation::from_bevy_translation(pos),
                            rotation: DclQuat::from_bevy_quat(rot),
                            scale: Vec3::ONE,
                            parent: SceneEntityId::WORLD_ORIGIN,
                        };

                        debug!("player: {:#x} -> {} -> {}", update.address, pos, vel);

                        state.update_crdt(
                            SceneComponentId::TRANSFORM,
                            CrdtType::LWW_ANY,
                            scene_id,
                            &dcl_transform,
                        );
                        position_events.write(PlayerPositionEvent {
                            index: None,
                            time: time.elapsed_secs(),
                            timestamp: Some(movement.temporal.timestamp_f32()),
                            player: entity,
                            translation: dcl_transform.translation,
                            rotation: dcl_transform.rotation,
                            velocity: Some(vel),
                            grounded: movement.temporal.grounded_or_err().ok(),
                            // Some(true) if either is Some(true), else Some(false) if either is Some(false), else None
                            jumping: movement
                                .temporal
                                .jump_or_err()
                                .ok()
                                .max(movement.temporal.long_jump_or_err().ok()),
                        });
                    }
                    PlayerMessage::PlayerData(Message::PlayerEmote(emote)) => {
                        debug!("emote: {emote:?}");
                        commands.entity(entity).try_insert(EmoteCommand {
                            urn: emote.urn.to_owned(),
                            timestamp: emote.incremental_id as i64,
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
        if player.last_update + 5.0 < time.elapsed_secs() {
            if let Ok(mut commands) = commands.get_entity(entity) {
                info!("removing stale player: {entity:?} : {player:?}");
                commands.despawn();
            }

            state.delete_entity(player.scene_id);
            state.lookup.remove_by_right(&entity);
        }
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

pub fn pipe_voice_to_scene(
    mut requests: EventReader<SystemApi>,
    sources: Query<(&ForeignPlayer, &ForeignAudioSource)>,
    mut senders: Local<Vec<RpcStreamSender<VoiceMessage>>>,
    mut current_active: Local<HashMap<ethers_core::types::Address, String>>,
    scene_rooms: Query<&SceneRoom>,
) {
    senders.extend(requests.read().filter_map(|ev| {
        if let SystemApi::GetVoiceStream(sender) = ev {
            Some(sender.clone())
        } else {
            None
        }
    }));

    senders.retain(|s| !s.is_closed());

    let mut prev_active = std::mem::take(&mut *current_active);

    for (source, audio) in sources.iter() {
        if let Some(transport) = audio.current_transport {
            let channel = match scene_rooms.get(transport).ok() {
                Some(room) => room.0.clone(),
                None => "Nearby".to_string(),
            };
            if prev_active.remove(&source.address).as_ref() != Some(&channel) {
                for sender in senders.iter() {
                    let _ = sender.send(VoiceMessage {
                        sender_address: format!("{:#x}", source.address),
                        channel: channel.clone(),
                        active: true,
                    });
                }
            }

            current_active.insert(source.address, channel);
        }
    }

    for (address, channel) in prev_active.drain() {
        for sender in senders.iter() {
            let _ = sender.send(VoiceMessage {
                sender_address: format!("{address:#x}"),
                channel: channel.clone(),
                active: false,
            });
        }
    }
}
