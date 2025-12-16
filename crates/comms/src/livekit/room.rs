use bevy::ecs::component::HookContext;
use bevy::prelude::*;
use bevy::{ecs::world::DeferredWorld, platform::collections::HashMap};
use common::structs::AudioDecoderError;
use common::util::AsH160;
use ethers_core::types::H160;
use http::Uri;
use kira::sound::streaming::StreamingSoundData;
use livekit::id::ParticipantIdentity;
#[cfg(not(target_arch = "wasm32"))]
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::{mpsc, oneshot};
#[cfg(not(target_arch = "wasm32"))]
use {
    livekit::{
        id::TrackSid,
        participant::{ConnectionQuality, Participant},
        Room, RoomEvent, RoomOptions, RoomResult,
    },
    {bevy::platform::sync::Arc, tokio::task::JoinHandle},
};
#[cfg(target_arch = "wasm32")]
use {
    tokio::sync::oneshot,
    wasm_bindgen::{
        convert::{FromWasmAbi, IntoWasmAbi},
        JsValue,
    },
    wasm_bindgen_futures::spawn_local,
};

use crate::global_crdt::ChannelControl;
use crate::livekit::participant::{HostingParticipants, LivekitParticipant};
use crate::livekit::plugin::{RoomTask, RoomTasks};
use crate::livekit::track::{Microphone, Publishing};
#[cfg(target_arch = "wasm32")]
use crate::livekit::web::{close_room, connect_room, recv_room_event, room_name, RoomEvent};
#[cfg(not(target_arch = "wasm32"))]
use crate::livekit::{participant, track};
use crate::livekit::{LivekitChannelControl, LivekitNetworkMessage};
use crate::livekit::{LivekitRuntime, LivekitTransport};
use crate::NetworkMessageRecipient;

#[cfg(target_arch = "wasm32")]
type JsValueAbi = <JsValue as IntoWasmAbi>::Abi;

#[derive(Component)]
pub struct LivekitRoom {
    pub room_name: String,
    #[cfg(not(target_arch = "wasm32"))]
    pub room: Arc<Room>,
    #[cfg(target_arch = "wasm32")]
    pub room: JsValueAbi,
    #[cfg(not(target_arch = "wasm32"))]
    pub room_event_receiver: mpsc::UnboundedReceiver<RoomEvent>,
}

impl LivekitRoom {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn get_room(&self) -> Arc<Room> {
        self.room.clone()
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for LivekitRoom {
    fn drop(&mut self) {
        // Build the value to drop the Abi memory
        let room = unsafe { JsValue::from_abi(self.room) };
        spawn_local(async move {
            let _room = room;
            // Just a bit of delay so that the call `close_room`
            // has time to finish
            futures_lite::future::yield_now().await;
        });
    }
}

/// Marks that a [`LivekitRoom`] as connected
#[derive(Component)]
#[component(on_add=Self::on_add, on_remove=Self::on_remove)]
pub struct Connected;

impl Connected {
    pub fn on_add(mut deferred_world: DeferredWorld, hook_context: HookContext) {
        let entity = hook_context.entity;
        debug!("Room {entity} connected.");

        deferred_world
            .commands()
            .entity(entity)
            .remove::<Connecting>();
    }

    pub fn on_remove(mut deferred_world: DeferredWorld, hook_context: HookContext) {
        let entity = hook_context.entity;

        // This hook will also run on despawn
        // so `try_remove` is used
        deferred_world
            .commands()
            .entity(entity)
            .try_remove::<LivekitRoom>();
    }
}

/// Marks that a [`LivekitRoom`] as connecting or
/// attempting to reconnect
#[derive(Component)]
#[component(on_add=Self::on_add, on_remove=Self::on_remove)]
pub struct Connecting;

impl Connecting {
    pub fn on_add(mut deferred_world: DeferredWorld, hook_context: HookContext) {
        let entity = hook_context.entity;
        debug!("Room {entity} is connecting.");

        deferred_world
            .commands()
            .entity(entity)
            .remove::<Connected>();
    }

    pub fn on_remove(mut deferred_world: DeferredWorld, hook_context: HookContext) {
        let entity = hook_context.entity;

        // This hook will also run on despawn
        // so `try_remove` is used
        deferred_world
            .commands()
            .entity(entity)
            .try_remove::<ConnectingLivekitRoom>();
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Default, Resource, Deref, DerefMut)]
struct LivekitRoomTrackTask(HashMap<TrackSid, JoinHandle<()>>);

#[derive(Component, Deref, DerefMut)]
struct ConnectingLivekitRoom(
    #[cfg(not(target_arch = "wasm32"))]
    JoinHandle<RoomResult<(Room, UnboundedReceiver<RoomEvent>)>>,
    #[cfg(target_arch = "wasm32")] oneshot::Receiver<anyhow::Result<JsValueAbi>>,
);

#[cfg(not(target_arch = "wasm32"))]
impl Drop for ConnectingLivekitRoom {
    fn drop(&mut self) {
        self.0.abort()
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for ConnectingLivekitRoom {
    fn drop(&mut self) {
        let (_, mut receiver) = oneshot::channel();
        std::mem::swap(&mut receiver, &mut self.0);
        if !receiver.is_terminated() {
            spawn_local(async move {
                if let Ok(Ok(js_value_abi)) = receiver.await {
                    let _ = unsafe { JsValue::from_abi(js_value_abi) };
                }
            })
        }
    }
}

pub(super) struct LivekitRoomPlugin;

impl Plugin for LivekitRoomPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(not(target_arch = "wasm32"))]
        app.init_resource::<LivekitRoomTrackTask>();

        app.add_observer(initiate_room_connection);
        app.add_observer(connect_to_livekit_room);
        app.add_observer(disconnect_from_room_on_replace);

        app.add_systems(
            Update,
            (
                poll_connecting_rooms,
                (
                    process_room_events,
                    process_channel_control,
                    process_network_message,
                ),
            )
                .chain(),
        );
    }
}

fn initiate_room_connection(trigger: Trigger<OnAdd, LivekitTransport>, mut commands: Commands) {
    commands.entity(trigger.target()).insert(Connecting);
}

fn connect_to_livekit_room(
    trigger: Trigger<OnAdd, Connecting>,
    mut commands: Commands,
    livekit_transports: Query<(&LivekitTransport, &LivekitRuntime)>,
) {
    let entity = trigger.target();
    #[cfg_attr(
        target_arch = "wasm32",
        expect(unused_variables, reason = "Runtime is used only on native")
    )]
    let Ok((livekit_transport, livekit_runtime)) = livekit_transports.get(entity) else {
        error!("{entity} does not have a LivekitRuntime.");
        return;
    };

    let remote_address = &livekit_transport.address;
    debug!(">> lk connect async : {remote_address}");

    let url = Uri::try_from(remote_address).unwrap();
    let address = format!(
        "{}://{}{}",
        url.scheme_str().unwrap_or_default(),
        url.host().unwrap_or_default(),
        url.path()
    );
    let params: HashMap<_, _, bevy::platform::hash::FixedHasher> =
        HashMap::from_iter(url.query().unwrap_or_default().split('&').flat_map(|par| {
            par.split_once('=')
                .map(|(a, b)| (a.to_owned(), b.to_owned()))
        }));
    debug!("{params:?}");
    let token = params.get("access_token").cloned().unwrap_or_default();

    #[cfg(not(target_arch = "wasm32"))]
    commands.entity(entity).insert(ConnectingLivekitRoom(
        livekit_runtime.spawn(connect_to_room(address, token)),
    ));
    #[cfg(target_arch = "wasm32")]
    {
        let (sender, receiver) = oneshot::channel();

        spawn_local(connect_to_room(address, token, sender));

        commands
            .entity(entity)
            .insert(ConnectingLivekitRoom(receiver));
    }
}

fn poll_connecting_rooms(
    mut commands: Commands,
    livekit_rooms: Populated<(Entity, &LivekitRuntime, &mut ConnectingLivekitRoom)>,
) {
    for (entity, livekit_runtime, mut connecting_livekit_room) in livekit_rooms.into_inner() {
        #[cfg(not(target_arch = "wasm32"))]
        let finished = connecting_livekit_room.is_finished();
        #[cfg(target_arch = "wasm32")]
        let finished = !connecting_livekit_room.is_empty();

        if finished {
            let Ok(poll) =
                livekit_runtime.block_on(connecting_livekit_room.as_deref_mut().as_mut())
            else {
                error!("Failed to poll ConnectingLivekitRoom.");
                continue;
            };

            match poll {
                #[cfg(not(target_arch = "wasm32"))]
                Ok((room, room_event_receiver)) => {
                    let local_participant = room.local_participant();

                    commands
                        .entity(entity)
                        .insert((
                            LivekitRoom {
                                room_name: room.name(),
                                room: Arc::new(room),
                                room_event_receiver,
                            },
                            Connected,
                        ))
                        .remove::<ConnectingLivekitRoom>();

                    commands.trigger(participant::ParticipantConnected {
                        participant: local_participant.into(),
                        room: entity,
                    });
                }
                #[cfg(target_arch = "wasm32")]
                Ok(room) => {
                    let js_room = unsafe { JsValue::from_abi(room) };
                    let room_name = room_name(&js_room);
                    // This prevents the memory for the object from being freed
                    let _ = js_room.into_abi();
                    commands
                        .entity(entity)
                        .insert((LivekitRoom { room_name, room }, Connected))
                        .remove::<ConnectingLivekitRoom>();
                }
                Err(err) => {
                    error!("Failed to connect to room due to '{err}'.");
                    commands.entity(entity).remove::<ConnectingLivekitRoom>();
                }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn connect_to_room(
    address: String,
    token: String,
) -> RoomResult<(Room, UnboundedReceiver<RoomEvent>)> {
    livekit::prelude::Room::connect(
        &address,
        &token,
        RoomOptions {
            auto_subscribe: false,
            adaptive_stream: false,
            dynacast: false,
            ..Default::default()
        },
    )
    .await
}

#[cfg(target_arch = "wasm32")]
async fn connect_to_room(
    address: String,
    token: String,
    sender: oneshot::Sender<anyhow::Result<JsValueAbi>>,
) {
    let res = connect_room(&address, &token)
        .await
        .map(IntoWasmAbi::into_abi)
        .map_err(|e| anyhow::anyhow!("Failed to connect room: {:?}", e));

    sender.send(res).unwrap();
}

fn process_room_events(
    mut commands: Commands,
    livekit_rooms: Query<(Entity, &mut LivekitRoom), With<Connected>>,
) {
    #[cfg_attr(target_arch = "wasm32", expect(unused_mut))]
    for (entity, mut livekit_room) in livekit_rooms {
        #[cfg(not(target_arch = "wasm32"))]
        let mut puller = || livekit_room.room_event_receiver.try_recv();
        #[cfg(target_arch = "wasm32")]
        let puller = || {
            let room = unsafe { JsValue::from_abi(livekit_room.room) };
            let head = recv_room_event(&room);
            let _ = room.into_abi();
            head.ok_or(mpsc::error::TryRecvError::Empty)
        };

        while let Ok(room_event) = puller() {
            trace!("in: {:?}", room_event);

            match room_event {
                #[cfg(not(target_arch = "wasm32"))]
                RoomEvent::Connected {
                    participants_with_tracks,
                } => {
                    for (participant, publications) in participants_with_tracks {
                        commands.trigger(participant::ParticipantConnected {
                            participant: participant.clone().into(),
                            room: entity,
                        });
                        for publication in &publications {
                            commands.trigger(track::TrackPublished {
                                participant: Participant::Remote(participant.clone()),
                                track: publication.clone(),
                            });
                        }
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                RoomEvent::DataReceived {
                    payload,
                    participant: maybe_participant,
                    ..
                } => {
                    if let Some(participant) = maybe_participant {
                        commands.trigger(participant::ParticipantPayload {
                            room: entity,
                            participant: participant.into(),
                            payload,
                        });
                    } else {
                        debug!("Owner-less payload received.");
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                RoomEvent::TrackPublished {
                    publication,
                    participant,
                } => {
                    commands.trigger(track::TrackPublished {
                        participant: Participant::Remote(participant.clone()),
                        track: publication.clone(),
                    });
                }
                #[cfg(not(target_arch = "wasm32"))]
                RoomEvent::TrackUnpublished {
                    publication,
                    participant,
                } => {
                    commands.trigger(track::TrackUnpublished {
                        participant: Participant::Remote(participant.clone()),
                        track: publication.clone(),
                    });
                }
                #[cfg(not(target_arch = "wasm32"))]
                RoomEvent::TrackSubscribed { publication, .. } => {
                    commands.trigger(track::TrackSubscribed { track: publication });
                }
                #[cfg(not(target_arch = "wasm32"))]
                RoomEvent::TrackUnsubscribed { publication, .. } => {
                    commands.trigger(track::TrackUnsubscribed { track: publication });
                }
                #[cfg(not(target_arch = "wasm32"))]
                RoomEvent::ParticipantConnected(participant) => {
                    commands.trigger(participant::ParticipantConnected {
                        participant: participant.clone().into(),
                        room: entity,
                    });
                }
                #[cfg(not(target_arch = "wasm32"))]
                RoomEvent::ParticipantDisconnected(participant) => {
                    commands.trigger(participant::ParticipantDisconnected {
                        participant: participant.into(),
                        room: entity,
                    });
                }
                #[cfg(not(target_arch = "wasm32"))]
                RoomEvent::ParticipantMetadataChanged { participant, .. } => {
                    commands.trigger(participant::ParticipantMetadataChanged {
                        room: entity,
                        participant: participant.into(),
                    });
                }
                #[cfg(not(target_arch = "wasm32"))]
                RoomEvent::ConnectionQualityChanged {
                    quality,
                    participant,
                } => match quality {
                    ConnectionQuality::Excellent => {
                        commands.trigger(participant::ParticipantConnectionQuality::new(
                            participant.into(),
                            entity,
                            participant::connection_quality::Excellent,
                        ));
                    }
                    ConnectionQuality::Good => {
                        commands.trigger(participant::ParticipantConnectionQuality::new(
                            participant.into(),
                            entity,
                            participant::connection_quality::Good,
                        ));
                    }
                    ConnectionQuality::Poor => {
                        commands.trigger(participant::ParticipantConnectionQuality::new(
                            participant.into(),
                            entity,
                            participant::connection_quality::Poor,
                        ));
                    }
                    ConnectionQuality::Lost => {
                        commands.trigger(participant::ParticipantConnectionQuality::new(
                            participant.into(),
                            entity,
                            participant::connection_quality::Lost,
                        ));
                    }
                },
                #[cfg(target_arch = "wasm32")]
                RoomEvent::DataReceived {
                    payload,
                    participant,
                    ..
                } => {
                    if let Some(address) = participant.identity.as_h160() {
                        if let Ok(packet) = rfc4::Packet::decode(payload.as_slice()) {
                            if let Some(message) = packet.message {
                                let _ = sender
                                    .try_send(PlayerUpdate {
                                        transport_id: entity,
                                        message: PlayerMessage::PlayerData(message),
                                        address,
                                    })
                                    .inspect_err(|err| {
                                        error!("Failed to send player update due to '{err}'")
                                    });
                            }
                        }
                    }
                }
                #[cfg(target_arch = "wasm32")]
                RoomEvent::TrackPublished {
                    participant, kind, ..
                } => {
                    debug!("pub {} {}", participant.identity, kind);
                    if let Some(address) = participant.identity.as_h160() {
                        if kind == "audio" {
                            let _ = sender
                                .try_send(PlayerUpdate {
                                    transport_id: entity,
                                    message: PlayerMessage::AudioStreamAvailable {
                                        transport: entity,
                                    },
                                    address,
                                })
                                .inspect_err(|err| {
                                    error!("Failed to send player update due to '{err}'")
                                });
                        }
                    }
                }
                #[cfg(target_arch = "wasm32")]
                RoomEvent::TrackUnpublished {
                    participant, kind, ..
                } => {
                    debug!("unpub {} {}", participant.identity, kind);
                    if let Some(address) = participant.identity.as_h160() {
                        if kind == "audio" {
                            let _ = sender
                                .try_send(PlayerUpdate {
                                    transport_id: entity,
                                    message: PlayerMessage::AudioStreamUnavailable {
                                        transport: entity,
                                    },
                                    address,
                                })
                                .inspect_err(|err| {
                                    error!("Failed to send player update due to '{err}'")
                                });
                        }
                    }
                }
                #[cfg(target_arch = "wasm32")]
                RoomEvent::TrackSubscribed { .. } => {
                    debug!("Track subscribed event - audio is handled in JavaScript");
                }
                #[cfg(target_arch = "wasm32")]
                RoomEvent::TrackUnsubscribed { .. } => {
                    debug!("Track unsubscribed event");
                }
                #[cfg(target_arch = "wasm32")]
                RoomEvent::ParticipantConnected { participant, .. } => {
                    if let Some(address) = participant.identity.as_h160() {
                        if !participant.metadata.is_empty() {
                            let _ = sender
                                .try_send(PlayerUpdate {
                                    transport_id: entity,
                                    message: PlayerMessage::MetaData(participant.metadata),
                                    address,
                                })
                                .inspect_err(|err| {
                                    error!("Failed to send player update due to '{err}'")
                                });
                        }
                    }
                }
                #[cfg(target_arch = "wasm32")]
                RoomEvent::ParticipantDisconnected { .. } => {
                    debug!("Participant disconnected");
                }
                #[cfg(not(target_arch = "wasm32"))]
                _ => {
                    debug!("Event: {:?}", room_event);
                }
            };
        }
    }
}

fn process_channel_control(
    mut commands: Commands,
    rooms: Query<(Entity, &mut LivekitChannelControl)>,
) {
    for (entity, mut channel_control) in rooms {
        loop {
            match channel_control.try_recv() {
                Ok(channel_control) => {
                    match channel_control {
                        ChannelControl::VoiceSubscribe(address, sender) => {
                            commands.run_system_cached_with(
                                subscribe_to_voice,
                                (entity, address, sender),
                            );
                        }
                        ChannelControl::VoiceUnsubscribe(address) => {
                            commands
                                .run_system_cached_with(unsubscribe_to_voice, (entity, address));
                        }
                        // ChannelControl::StreamerSubscribe(audio, video) => {
                        //     streamer_audio_subscribe(
                        //         &livekit_room,
                        //         Some(audio),
                        //         &mut streamer_audio_channel,
                        //     )
                        //     .await;
                        //     streamer_video_subscribe(
                        //         &livekit_room,
                        //         Some(video),
                        //         &mut streamer_video_channel,
                        //     )
                        //     .await;
                        // }
                        // ChannelControl::StreamerUnsubscribe => {
                        //     streamer_audio_subscribe(
                        //         &livekit_room,
                        //         None,
                        //         &mut streamer_audio_channel,
                        //     )
                        //     .await;
                        //     streamer_video_subscribe(
                        //         &livekit_room,
                        //         None,
                        //         &mut streamer_video_channel,
                        //     )
                        //     .await;
                        // }
                        _ => continue,
                    };
                    // channel_control_tasks.push(ChannelControlTask {
                    //     runtime: runtime.clone(),
                    //     task,
                    // });
                }
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    error!("Channel control of {entity} was closed.");
                    commands.send_event(AppExit::from_code(1));
                    return;
                }
            }
        }
    }
}

fn process_network_message(
    mut commands: Commands,
    rooms: Query<(
        Entity,
        &LivekitRoom,
        &LivekitRuntime,
        &mut LivekitNetworkMessage,
    )>,
    mut room_tasks: ResMut<RoomTasks>,
) {
    for (entity, room, runtime, mut network_message) in rooms {
        loop {
            match network_message.try_recv() {
                Ok(outgoing) => {
                    let destination_identities = match outgoing.recipient {
                        NetworkMessageRecipient::All => Vec::default(),
                        NetworkMessageRecipient::Peer(address) => {
                            vec![ParticipantIdentity(format!("{address:#x}"))]
                        }
                        NetworkMessageRecipient::AuthServer => {
                            vec![ParticipantIdentity("authoritative-server".to_string())]
                        }
                    };

                    let packet = livekit::DataPacket {
                        payload: outgoing.data,
                        topic: None,
                        reliable: !outgoing.unreliable,
                        destination_identities,
                    };

                    let local_participant = room.room.local_participant();
                    let task =
                        runtime.spawn(async move { local_participant.publish_data(packet).await });
                    room_tasks.push(RoomTask {
                        runtime: runtime.clone(),
                        task,
                    });
                }
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    error!("Network message of {entity} was closed.");
                    commands.send_event(AppExit::from_code(1));
                    return;
                }
            }
        }
    }
}

fn disconnect_from_room_on_replace(
    trigger: Trigger<OnReplace, LivekitRoom>,
    livekit_rooms: Query<(&LivekitRoom, Option<&LivekitRuntime>)>,
) {
    let entity = trigger.target();
    #[cfg_attr(
        target_arch = "wasm32",
        expect(unused_variables, reason = "Runtime is used only on native")
    )]
    let Ok((livekit_room, maybe_livekit_runtime)) = livekit_rooms.get(entity) else {
        unreachable!("Infallible query.");
    };

    #[cfg(not(target_arch = "wasm32"))]
    {
        let room = livekit_room.room.clone();
        debug!("Closing room {}.", room.name());
        if let Some(runtime) = maybe_livekit_runtime {
            runtime.spawn(async move {
                if let Err(err) = room.close().await {
                    error!("Error while closing room {}. '{err}'.", room.name());
                }
            });
        } else {
            warn!("Closing a room in blocking context because LivekitRoom does not have a LivekitRuntime.");
            tokio::task::spawn_blocking(async move || {
                if let Err(err) = room.close().await {
                    error!("Error while closing room {}. '{err}'.", room.name());
                }
            });
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let room = unsafe { JsValue::from_abi(livekit_room.room) };
        let room_name = room_name(&room);
        debug!("Closing room {}.", room_name);
        spawn_local(async move {
            if let Err(err) = close_room(&room).await {
                error!("Error while closing room {}. '{err:?}'.", room_name);
            }
            // Prevent the Javascript memory from being freed just yet
            room.into_abi();
        });
    }
}

fn subscribe_to_voice(
    In((room_entity, address, sender)): In<(
        Entity,
        H160,
        oneshot::Sender<StreamingSoundData<AudioDecoderError>>,
    )>,
    mut commands: Commands,
    rooms: Query<(&LivekitRoom, &LivekitRuntime, Option<&HostingParticipants>)>,
    participants: Query<(&LivekitParticipant, &Publishing)>,
    tracks: Query<Entity, With<Microphone>>,
) {
    let Ok((room, runtime, maybe_hosting)) = rooms.get(room_entity) else {
        error!("{} is not an well formed room.", room_entity);
        commands.send_event(AppExit::from_code(1));
        return;
    };

    let Some(hosting) = maybe_hosting else {
        error!(
            "Trying to subscribe to voice in room {}, but there are not participants.",
            room.room_name
        );
        return;
    };

    let Some((participant, publishing)) =
        participants
            .iter_many(hosting.collection())
            .find(|(participant, _)| {
                participant
                    .identity()
                    .as_str()
                    .as_h160()
                    .filter(|participant_address| participant_address == &address)
                    .is_some()
            })
    else {
        error!(
            "No participant with address {} in room {}.",
            address, room.room_name
        );
        return;
    };

    if let Some(track_entity) = tracks.iter_many(publishing.collection()).next() {
        commands.trigger_targets(
            track::SubscribeToTrack {
                runtime: runtime.clone(),
                sender,
            },
            track_entity,
        );
    } else {
        error!(
            "No microphone track for {} ({}).",
            participant.sid(),
            participant.identity()
        );
    }
}

fn unsubscribe_to_voice(
    In((room_entity, address)): In<(Entity, H160)>,
    mut commands: Commands,
    rooms: Query<(&LivekitRoom, &LivekitRuntime, Option<&HostingParticipants>)>,
    participants: Query<(&LivekitParticipant, &Publishing)>,
    tracks: Query<Entity, With<Microphone>>,
) {
    let Ok((room, runtime, maybe_hosting)) = rooms.get(room_entity) else {
        error!("{} is not an well formed room.", room_entity);
        commands.send_event(AppExit::from_code(1));
        return;
    };

    let Some(hosting) = maybe_hosting else {
        error!(
            "Trying to subscribe to voice in room {}, but there are not participants.",
            room.room_name
        );
        return;
    };

    let Some((participant, publishing)) =
        participants
            .iter_many(hosting.collection())
            .find(|(participant, _)| {
                participant
                    .identity()
                    .as_str()
                    .as_h160()
                    .filter(|participant_address| participant_address == &address)
                    .is_some()
            })
    else {
        error!(
            "No participant with address {} in room {}.",
            address, room.room_name
        );
        return;
    };

    if let Some(track_entity) = tracks.iter_many(publishing.collection()).next() {
        commands.trigger_targets(
            track::UnsubscribeToTrack {
                runtime: runtime.clone(),
            },
            track_entity,
        );
    } else {
        error!(
            "No microphone track for {} ({}).",
            participant.sid(),
            participant.identity()
        );
    }
}
