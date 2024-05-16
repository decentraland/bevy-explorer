use std::ops::RangeInclusive;

use bevy::{
    prelude::*,
    utils::{HashMap, HashSet},
};
use bimap::BiMap;
use common::{
    rpc::{RpcCall, RpcEventSender},
    structs::{AttachPoints, AudioDecoderError, AvatarTextureHandle},
};
use ethers_core::types::Address;
use kira::sound::streaming::StreamingSoundData;
use serde_json::json;
use tokio::sync::{broadcast, mpsc};

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

const FOREIGN_PLAYER_RANGE: RangeInclusive<u16> = 6..=406;

pub struct GlobalCrdtPlugin;

impl Plugin for GlobalCrdtPlugin {
    fn build(&self, app: &mut App) {
        let (ext_sender, ext_receiver) = mpsc::channel(1000);
        let (int_sender, int_receiver) = broadcast::channel(1000);
        // leak the receiver so it never gets dropped
        Box::leak(Box::new(int_receiver));
        app.insert_resource(GlobalCrdtState {
            ext_receiver,
            ext_sender,
            int_sender,
            context: CrdtContext::new(SceneId::DUMMY, "Global Crdt".into(), false, false),
            store: Default::default(),
            lookup: Default::default(),
        });

        let (sender, receiver) = tokio::sync::broadcast::channel(1_000);
        // leak the receiver so it never gets dropped
        Box::leak(Box::new(receiver));
        app.insert_resource(LocalAudioSource { sender });

        app.add_systems(Update, process_transport_updates);
        app.add_systems(Update, despawn_players);
        app.add_event::<PlayerPositionEvent>();
        app.add_event::<ProfileEvent>();
        app.add_event::<ChatEvent>();
    }
}

pub enum PlayerMessage {
    PlayerData(rfc4::packet::Message),
    AudioStream(Box<StreamingSoundData<AudioDecoderError>>),
}

impl std::fmt::Debug for PlayerMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerData(arg0) => f.debug_tuple("PlayerData").field(arg0).finish(),
            Self::AudioStream(_) => f.debug_tuple("AudioStream").finish(),
        }
    }
}

#[derive(Debug)]
pub struct PlayerUpdate {
    pub transport_id: Entity,
    pub message: PlayerMessage,
    pub address: Address,
}

#[derive(Resource)]
pub struct GlobalCrdtState {
    // receiver from sockets
    ext_receiver: mpsc::Receiver<PlayerUpdate>,
    // sender for sockets to post to
    ext_sender: mpsc::Sender<PlayerUpdate>,
    // sender for broadcast updates
    int_sender: broadcast::Sender<Vec<u8>>,
    // receiver for broadcast updates (we keep it to ensure it doesn't get closed)
    context: CrdtContext,
    store: CrdtStore,
    lookup: BiMap<Address, Entity>,
}

impl GlobalCrdtState {
    // get a channel to which updates can be sent
    pub fn get_sender(&self) -> mpsc::Sender<PlayerUpdate> {
        self.ext_sender.clone()
    }

    // get a channel from which crdt updates can be received
    pub fn subscribe(&self) -> (CrdtStore, broadcast::Receiver<Vec<u8>>) {
        (self.store.clone(), self.int_sender.subscribe())
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
    audio_sender: mpsc::Sender<StreamingSoundData<AudioDecoderError>>,
}

#[derive(Component)]
pub struct ForeignAudioSource(pub mpsc::Receiver<StreamingSoundData<AudioDecoderError>>);

// TODO: I should avoid the clone on recv somehow
#[derive(Clone)]
pub struct LocalAudioFrame {
    pub data: Vec<f32>,
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

#[derive(Event)]
pub struct PlayerPositionEvent {
    pub index: u32,
    pub player: Entity,
    pub time: f32,
    pub translation: DclTranslation,
    pub rotation: DclQuat,
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

#[derive(Event)]
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
    mut binary_senders: Local<
        HashMap<String, tokio::sync::mpsc::UnboundedSender<(String, Vec<u8>)>>,
    >,
    mut subscribers: EventReader<RpcCall>,
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
        (
            Entity,
            SceneEntityId,
            mpsc::Sender<StreamingSoundData<AudioDecoderError>>,
        ),
    > = HashMap::default();

    while let Ok(update) = state.ext_receiver.try_recv() {
        // create/update timestamp/transport_id on the foreign player
        let (entity, scene_id, audio_channel) =
            if let Some((entity, scene_id, channel)) = created_this_frame.get(&update.address) {
                (*entity, *scene_id, channel.clone())
            } else if let Some(existing) = state.lookup.get_by_left(&update.address) {
                let mut foreign_player = players.get_mut(*existing).unwrap();
                foreign_player.last_update = time.elapsed_seconds();
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

                let (audio_sender, audio_receiver) = mpsc::channel(1);

                let attach_points = AttachPoints::new(&mut commands);

                let new_entity = commands
                    .spawn((
                        SpatialBundle::default(),
                        ForeignPlayer {
                            address: update.address,
                            transport_id: update.transport_id,
                            last_update: time.elapsed_seconds(),
                            scene_id: next_free,
                            profile_version: 0,
                            audio_sender: audio_sender.clone(),
                        },
                        ForeignAudioSource(audio_receiver),
                        AvatarTextureHandle::default(),
                    ))
                    .push_children(&attach_points.entities())
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
            PlayerMessage::AudioStream(audio) => {
                // pass through
                let _ = audio_channel.blocking_send(*audio);
            }
            PlayerMessage::PlayerData(Message::Position(pos)) => {
                let dcl_transform = DclTransformAndParent {
                    translation: DclTranslation([pos.position_x, pos.position_y, pos.position_z]),
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
                position_events.send(PlayerPositionEvent {
                    index: pos.index,
                    time: time.elapsed_seconds(),
                    player: entity,
                    translation: DclTranslation([pos.position_x, pos.position_y, pos.position_z]),
                    rotation: DclQuat([
                        pos.rotation_x,
                        pos.rotation_y,
                        pos.rotation_z,
                        pos.rotation_w,
                    ]),
                });
            }
            PlayerMessage::PlayerData(Message::ProfileVersion(version)) => {
                profile_events.send(ProfileEvent {
                    sender: entity,
                    event: ProfileEventType::Version(version),
                });
            }
            PlayerMessage::PlayerData(Message::ProfileRequest(request)) => {
                profile_events.send(ProfileEvent {
                    sender: entity,
                    event: ProfileEventType::Request(request),
                });
            }
            PlayerMessage::PlayerData(Message::ProfileResponse(response)) => {
                profile_events.send(ProfileEvent {
                    sender: entity,
                    event: ProfileEventType::Response(response),
                });
            }
            PlayerMessage::PlayerData(Message::Chat(chat)) => {
                chat_events.send(ChatEvent {
                    sender: entity,
                    timestamp: chat.timestamp,
                    channel: "Nearby".to_owned(),
                    message: chat.message,
                });
            }
            PlayerMessage::PlayerData(Message::Scene(mut scene)) => {
                if scene.data.is_empty() {
                    warn!("empty scene message");
                    continue;
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
                    update.address, scene.scene_id, comms_type, scene.data
                );

                match comms_type {
                    CommsMessageType::String => {
                        if let Some(sender) = string_senders.get(&scene.scene_id) {
                            let _ = sender.send(
                                json!({
                                    "message": String::from_utf8(scene.data).unwrap_or_default(),
                                    "sender": format!("{:#x}", update.address),
                                })
                                .to_string(),
                            );
                        }
                    }
                    CommsMessageType::Binary => {
                        if let Some(sender) = binary_senders.get(&scene.scene_id) {
                            let _ = sender.send((format!("{:#x}", update.address), scene.data));
                        }
                    }
                }
            }
            PlayerMessage::PlayerData(Message::Voice(_)) => (),
            PlayerMessage::PlayerData(Message::Movement(_)) => (),
            PlayerMessage::PlayerData(Message::PlayerEmote(_)) => (),
            PlayerMessage::PlayerData(Message::SceneEmote(_)) => (),
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
        if player.last_update + 5.0 < time.elapsed_seconds() {
            if let Some(commands) = commands.get_entity(entity) {
                info!("removing stale player: {entity:?} : {player:?}");
                commands.despawn_recursive();
            }

            state.delete_entity(player.scene_id);
            state.lookup.remove_by_right(&entity);
        }
    }
}
