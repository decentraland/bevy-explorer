use bevy::{
    platform::{collections::HashMap, sync::Arc},
    prelude::*,
};
use common::{structs::AudioDecoderError, util::AsH160};
use ethers_core::types::H160;
use http::Uri;
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};
#[cfg(not(target_arch = "wasm32"))]
use {
    kira::sound::streaming::StreamingSoundData,
    livekit::{
        id::ParticipantIdentity, participant::Participant, ConnectionState, DataPacket, Room,
        RoomError, RoomEvent, RoomOptions, RoomResult,
    },
};

use crate::{
    global_crdt::ChannelControl,
    livekit::{
        participant::{
            HostingParticipants, LivekitParticipant, ParticipantConnected,
            ParticipantConnectionQuality, ParticipantDisconnected, ParticipantMetadataChanged,
            ParticipantPayload,
        },
        room::{
            Connected, Connecting, ConnectingLivekitRoom, Disconnected, LivekitRoom, Reconnecting,
        },
        track, LivekitChannelControl, LivekitNetworkMessage, LivekitRuntime, LivekitTransport,
    },
    NetworkMessageRecipient,
};
#[cfg(target_arch = "wasm32")]
use crate::{
    global_crdt::StreamingSoundData,
    livekit::web::{
        ConnectionState, DataPacket, Participant, ParticipantIdentity, Room, RoomError, RoomEvent,
        RoomOptions, RoomResult,
    },
};

pub struct LivekitRoomPlugin;

impl Plugin for LivekitRoomPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(livekit_transport_added);
        app.add_observer(initiate_room_connection);
        app.add_observer(create_local_participant);
        app.add_observer(disconnect_from_room_on_replace);

        app.add_systems(
            Update,
            (
                try_reconnect,
                poll_connecting_rooms,
                (
                    process_room_events,
                    process_channel_control,
                    process_network_message,
                ),
                verify_room_tasks,
            )
                .chain(),
        );
        app.add_systems(Last, close_rooms_on_app_exit.run_if(on_event::<AppExit>));
    }
}

#[derive(Default, Component, Deref, DerefMut)]
struct RoomTasks(Vec<RoomTask>);

#[derive(Deref, DerefMut)]
struct RoomTask(JoinHandle<Result<(), RoomError>>);

fn livekit_transport_added(trigger: Trigger<OnAdd, LivekitTransport>, mut commands: Commands) {
    let entity = trigger.target();
    commands.entity(entity).insert(Connecting);
}

fn initiate_room_connection(
    trigger: Trigger<OnAdd, Connecting>,
    mut commands: Commands,
    livekit_transports: Query<&LivekitTransport>,
    livekit_runtime: Res<LivekitRuntime>,
) {
    let entity = trigger.target();
    let Ok(livekit_transport) = livekit_transports.get(entity) else {
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

    commands.entity(entity).insert(ConnectingLivekitRoom(
        livekit_runtime.spawn(connect_to_room(address, token)),
    ));
}

fn poll_connecting_rooms(
    mut commands: Commands,
    livekit_rooms: Populated<(Entity, &mut ConnectingLivekitRoom)>,
    livekit_runtime: Res<LivekitRuntime>,
) {
    for (entity, mut connecting_livekit_room) in livekit_rooms.into_inner() {
        if connecting_livekit_room.is_finished() {
            let Ok(poll) =
                livekit_runtime.block_on(connecting_livekit_room.as_deref_mut().as_mut())
            else {
                error!("Failed to poll ConnectingLivekitRoom.");
                continue;
            };

            match poll {
                Ok((room, room_event_receiver)) => {
                    commands
                        .entity(entity)
                        .insert(LivekitRoom {
                            room: Arc::new(room),
                            room_event_receiver,
                        })
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

async fn connect_to_room(
    address: String,
    token: String,
) -> RoomResult<(Room, mpsc::UnboundedReceiver<RoomEvent>)> {
    Room::connect(
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

fn process_room_events(mut commands: Commands, livekit_rooms: Query<(Entity, &mut LivekitRoom)>) {
    for (entity, mut livekit_room) in livekit_rooms {
        while let Ok(room_event) = livekit_room.room_event_receiver.try_recv() {
            trace!("in: {:?}", room_event);

            match room_event {
                RoomEvent::Connected {
                    participants_with_tracks,
                } => {
                    for (participant, publications) in participants_with_tracks {
                        commands.trigger(ParticipantConnected {
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
                RoomEvent::ConnectionStateChanged(state) => match state {
                    ConnectionState::Connected => {
                        commands.entity(entity).insert(Connected);
                    }
                    ConnectionState::Reconnecting => {
                        commands.entity(entity).insert(Reconnecting);
                    }
                    ConnectionState::Disconnected => {
                        commands.entity(entity).insert(Disconnected);
                    }
                },
                RoomEvent::DataReceived {
                    payload,
                    participant: maybe_participant,
                    ..
                } => {
                    if let Some(participant) = maybe_participant {
                        commands.trigger(ParticipantPayload {
                            room: entity,
                            participant: participant.into(),
                            payload,
                        });
                    } else {
                        debug!("Owner-less payload received.");
                    }
                }
                RoomEvent::ParticipantConnected(participant) => {
                    commands.trigger(ParticipantConnected {
                        participant: participant.clone().into(),
                        room: entity,
                    });
                }
                RoomEvent::ParticipantDisconnected(participant) => {
                    commands.trigger(ParticipantDisconnected {
                        participant: participant.into(),
                        room: entity,
                    });
                }
                RoomEvent::ParticipantMetadataChanged { participant, .. } => {
                    commands.trigger(ParticipantMetadataChanged {
                        room: entity,
                        participant: participant.into(),
                    });
                }
                RoomEvent::TrackPublished {
                    publication,
                    participant,
                } => {
                    commands.trigger(track::TrackPublished {
                        participant: Participant::Remote(participant.clone()),
                        track: publication.clone(),
                    });
                }
                RoomEvent::TrackUnpublished {
                    publication,
                    participant,
                } => {
                    commands.trigger(track::TrackUnpublished {
                        participant: Participant::Remote(participant.clone()),
                        track: publication.clone(),
                    });
                }
                RoomEvent::TrackSubscribed { publication, .. } => {
                    commands.trigger(track::TrackSubscribed { track: publication });
                }
                RoomEvent::TrackUnsubscribed { publication, .. } => {
                    commands.trigger(track::TrackUnsubscribed { track: publication });
                }
                RoomEvent::ConnectionQualityChanged {
                    quality,
                    participant,
                } => {
                    commands.trigger(ParticipantConnectionQuality::new(
                        participant.into(),
                        entity,
                        quality,
                    ));
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
    rooms: Query<(Entity, &LivekitRoom, &mut LivekitChannelControl)>,
) {
    for (entity, livekit_room, mut channel_control) in rooms {
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
                    };
                }
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    error!("Channel control of {} was closed.", livekit_room.name());
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
        &mut LivekitNetworkMessage,
        Option<&mut RoomTasks>,
    )>,
    livekit_runtime: Res<LivekitRuntime>,
) {
    let mut new_room_tasks = vec![];
    for (entity, room, mut network_message, maybe_room_tasks) in rooms {
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

                    let packet = DataPacket {
                        payload: outgoing.data,
                        topic: None,
                        reliable: !outgoing.unreliable,
                        destination_identities,
                    };

                    let local_participant = room.local_participant();
                    let task = livekit_runtime
                        .spawn(async move { local_participant.publish_data(packet).await });
                    new_room_tasks.push(RoomTask(task));
                }
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    error!("Network message of {} was closed.", room.name());
                    commands.send_event(AppExit::from_code(1));
                    return;
                }
            }
        }
        if let Some(mut room_tasks) = maybe_room_tasks {
            room_tasks.extend(&mut new_room_tasks.drain(..));
        } else {
            #[expect(
                clippy::drain_collect,
                reason = "This does not reset the capacity of `new_room_tasks`."
            )]
            commands
                .entity(entity)
                .insert(RoomTasks(new_room_tasks.drain(..).collect()));
        }
    }
}

fn create_local_participant(
    trigger: Trigger<OnAdd, Connected>,
    mut commands: Commands,
    rooms: Query<&LivekitRoom>,
) {
    let entity = trigger.target();
    let Ok(room) = rooms.get(entity) else {
        error!("Can't create local participant because {entity} is not a LivekitRoom.");
        commands.send_event(AppExit::from_code(1));
        return;
    };

    let local_participant = room.local_participant();
    commands.trigger(ParticipantConnected {
        participant: local_participant.into(),
        room: entity,
    });
}

fn disconnect_from_room_on_replace(
    trigger: Trigger<OnReplace, LivekitRoom>,
    livekit_rooms: Query<&LivekitRoom>,
    livekit_runtime: Res<LivekitRuntime>,
) {
    let entity = trigger.target();
    let Ok(livekit_room) = livekit_rooms.get(entity) else {
        unreachable!("Infallible query.");
    };

    let room = livekit_room.room.clone();
    debug!("Closing room {}.", room.name());
    livekit_runtime.spawn(async move {
        if let Err(err) = room.close().await {
            error!("Error while closing room {}. '{err}'.", room.name());
        }
    });
}

type SubscribeToAudio = (
    Entity,
    H160,
    oneshot::Sender<StreamingSoundData<AudioDecoderError>>,
);

fn subscribe_to_voice(
    In(input): In<SubscribeToAudio>,
    mut commands: Commands,
    rooms: Query<(&LivekitRoom, Option<&HostingParticipants>)>,
    participants: Query<(&LivekitParticipant, &track::Publishing)>,
    tracks: Query<Entity, With<track::Microphone>>,
) {
    #[cfg(not(target_arch = "wasm32"))]
    let (room_entity, address, sender) = input;
    #[cfg(target_arch = "wasm32")]
    let (room_entity, address, _) = input;

    let Ok((room, maybe_hosting)) = rooms.get(room_entity) else {
        error!("{} is not an well formed room.", room_entity);
        commands.send_event(AppExit::from_code(1));
        return;
    };

    let Some(hosting) = maybe_hosting else {
        error!(
            "Trying to subscribe to voice in room {}, but there are not participants.",
            room.name()
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
            address,
            room.name()
        );
        return;
    };

    if let Some(track_entity) = tracks.iter_many(publishing.collection()).next() {
        commands.trigger_targets(
            track::SubscribeToAudioTrack {
                #[cfg(not(target_arch = "wasm32"))]
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
    rooms: Query<(&LivekitRoom, Option<&HostingParticipants>)>,
    participants: Query<(&LivekitParticipant, &track::Publishing)>,
    tracks: Query<Entity, With<track::Microphone>>,
) {
    let Ok((room, maybe_hosting)) = rooms.get(room_entity) else {
        error!("{} is not an well formed room.", room_entity);
        commands.send_event(AppExit::from_code(1));
        return;
    };

    let Some(hosting) = maybe_hosting else {
        error!(
            "Trying to subscribe to voice in room {}, but there are not participants.",
            room.name()
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
            address,
            room.name()
        );
        return;
    };

    if let Some(track_entity) = tracks.iter_many(publishing.collection()).next() {
        commands.trigger_targets(track::UnsubscribeToTrack, track_entity);
    } else {
        error!(
            "No microphone track for {} ({}).",
            participant.sid(),
            participant.identity()
        );
    }
}

fn verify_room_tasks(
    mut commands: Commands,
    rooms: Query<&mut RoomTasks, With<LivekitRoom>>,
    livekit_runtime: Res<LivekitRuntime>,
) {
    for mut room_tasks in rooms {
        let mut i = 0;
        while i < room_tasks.len() {
            if room_tasks[i].is_finished() {
                let RoomTask(task) = room_tasks.swap_remove(i);

                let res = livekit_runtime.block_on(task);
                match res {
                    Ok(Ok(())) => {}
                    Ok(Err(err)) => {
                        error!("Failed to complete room task due to {err}.");
                    }
                    Err(err) => {
                        error!("Failed to pull RoomTask due to '{err}'.");
                        commands.send_event(AppExit::from_code(1));
                        return;
                    }
                }
            } else {
                i += 1;
            }
        }
    }
}

fn close_rooms_on_app_exit(rooms: Query<&LivekitRoom>, livekit_runtime: Res<LivekitRuntime>) {
    for room in rooms {
        if let Err(err) = livekit_runtime.block_on(room.close()) {
            error!(
                "Failed to close room {} before exiting due to '{err}'.",
                room.name()
            );
        }
    }
}

fn try_reconnect(mut commands: Commands, rooms: Populated<Entity, With<Disconnected>>) {
    for entity in rooms.into_inner() {
        commands.entity(entity).insert(Connecting);
    }
}
