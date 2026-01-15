use bevy::{
    asset::RenderAssetUsages,
    color::palettes,
    ecs::relationship::Relationship,
    prelude::*,
    render::render_resource::{TextureDimension, TextureFormat, TextureUsages},
};
use common::util::AsH160;
use dcl_component::proto_components::kernel::comms::rfc4;
#[cfg(not(target_arch = "wasm32"))]
use livekit::prelude::Participant;
use prost::Message;

#[cfg(target_arch = "wasm32")]
use crate::livekit::web::Participant;
use crate::{
    global_crdt::{GlobalCrdtState, NonPlayerUpdate, PlayerMessage, PlayerUpdate},
    livekit::{
        participant::{
            HostedBy, HostingParticipants, LivekitParticipant, Local, ParticipantConnected,
            ParticipantConnectionQuality, ParticipantDisconnected, ParticipantMetadataChanged,
            ParticipantPayload, StreamBroadcast, StreamImage, StreamViewer, Streamer,
        },
        plugin::{PlayerUpdateTask, PlayerUpdateTasks},
        room::LivekitRoom,
        track::{Audio, LivekitTrack, Publishing, SubscribeToTrack, Video},
        LivekitRuntime,
    },
};

pub struct LivekitParticipantPlugin;

impl Plugin for LivekitParticipantPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(participant_connected);
        app.add_observer(participant_disconnected);
        app.add_observer(participant_connection_quality_changed);
        app.add_observer(participant_payload);
        app.add_observer(participant_metadata_changed);

        app.add_systems(
            Update,
            (
                stream_viewer_without_stream_image,
                non_stream_viewer_with_stream_image,
            ),
        );
        app.add_observer(someone_wants_to_watch_stream);
        app.add_observer(noone_is_watching_stream);
    }
}

fn participant_connected(
    trigger: Trigger<ParticipantConnected>,
    mut commands: Commands,
    rooms: Query<&LivekitRoom>,
) {
    let ParticipantConnected {
        participant,
        room: room_entity,
    } = trigger.event();
    let Ok(room) = rooms.get(*room_entity) else {
        error!("Room {room_entity} given to ParticipantConnected was invalid.");
        commands.send_event(AppExit::from_code(1));
        return;
    };
    debug!(
        "Participant '{}' ({}) connected to room {}.",
        participant.sid(),
        participant.identity(),
        room.name()
    );

    let is_local = matches!(participant.participant, Participant::Local(_));

    if is_local {
        commands.spawn((
            participant.clone(),
            <HostedBy as Relationship>::from(*room_entity),
            Local,
        ));
    } else if participant.identity().as_str().ends_with("-streamer") {
        commands.spawn((
            participant.clone(),
            <HostedBy as Relationship>::from(*room_entity),
            Streamer,
        ));
    } else {
        commands.spawn((
            participant.clone(),
            <HostedBy as Relationship>::from(*room_entity),
        ));
    }

    commands.trigger(ParticipantMetadataChanged {
        room: *room_entity,
        participant: participant.clone(),
    });
}

fn participant_disconnected(
    trigger: Trigger<ParticipantDisconnected>,
    mut commands: Commands,
    participants: Query<(Entity, &LivekitParticipant)>,
    rooms: Query<(&LivekitRoom, Option<&HostingParticipants>)>,
) {
    let ParticipantDisconnected {
        participant,
        room: room_entity,
    } = trigger.event();
    let Ok((room, maybe_hosting_participants)) = rooms.get(*room_entity) else {
        error!("Room {room_entity} given to ParticipantDisconnected was invalid.");
        commands.send_event(AppExit::from_code(1));
        return;
    };
    debug!(
        "Participant '{}' ({}) disconnected from room {}.",
        participant.sid(),
        participant.identity(),
        room.name()
    );

    let Some(hosting_participants) = maybe_hosting_participants else {
        error!("Room {} is not hosting participants.", room.name());
        commands.send_event(AppExit::from_code(1));
        return;
    };

    let Some(entity) = participants
        .iter_many(hosting_participants.collection())
        .find_map(|(entity, ecs_participant)| {
            if ecs_participant.sid() == participant.sid() {
                Some(entity)
            } else {
                None
            }
        })
    else {
        error!(
            "Disconnecting participant '{}' ({}) not found in participants.",
            participant.sid(),
            participant.identity()
        );
        return;
    };

    commands.entity(entity).despawn();
}

fn participant_connection_quality_changed(
    trigger: Trigger<ParticipantConnectionQuality>,
    mut commands: Commands,
    participants: Query<(Entity, &LivekitParticipant)>,
    rooms: Query<&HostingParticipants, With<LivekitRoom>>,
) {
    let ParticipantConnectionQuality {
        participant,
        room,
        connection_quality,
    } = trigger.event();
    debug!(
        "Participant '{}' ({}) connection quality with {room} changed to {:?}.",
        participant.sid(),
        participant.identity(),
        connection_quality
    );

    let Ok(hosting_participants) = rooms.get(*room) else {
        error!("Room given to ParticipantDisconnected was invalid.");
        commands.send_event(AppExit::from_code(1));
        return;
    };

    let Some(entity) = participants
        .iter_many(hosting_participants.collection())
        .find_map(|(entity, ecs_participant)| {
            if ecs_participant.sid() == participant.sid() {
                Some(entity)
            } else {
                None
            }
        })
    else {
        error!(
            "No entity referent to '{}' ({}).",
            participant.sid(),
            participant.identity()
        );
        return;
    };

    commands.entity(entity).insert(*connection_quality);
}

fn participant_payload(
    trigger: Trigger<ParticipantPayload>,
    global_crdt_state: Res<GlobalCrdtState>,
    mut player_update_tasks: ResMut<PlayerUpdateTasks>,
    livekit_runtime: Res<LivekitRuntime>,
) {
    let ParticipantPayload {
        room: room_entity,
        participant,
        payload,
    } = trigger.event();

    let packet = match rfc4::Packet::decode(payload.as_slice()) {
        Ok(packet) => packet,
        Err(_) => {
            warn!(
                "Could not decode payload from participant {} ({}).",
                participant.sid(),
                participant.identity()
            );
            return;
        }
    };
    let Some(message) = packet.message else {
        warn!(
            "Payload from {} ({}) had empty body.",
            participant.sid(),
            participant.identity()
        );
        return;
    };
    let room = *room_entity;
    let sender = global_crdt_state.get_sender();

    let task = if let Some(address) = participant.identity().as_str().as_h160() {
        trace!(
            "[{}] received [{}] packet {message:?} from {address}",
            room_entity,
            packet.protocol_version
        );
        livekit_runtime.spawn(async move {
            sender
                .send(
                    PlayerUpdate {
                        transport_id: room,
                        message: PlayerMessage::PlayerData(message),
                        address,
                    }
                    .into(),
                )
                .await
        })
    } else {
        let address = participant.identity().to_string();
        livekit_runtime.spawn(async move {
            sender
                .send(
                    NonPlayerUpdate {
                        transport_id: room,
                        address,
                        message,
                    }
                    .into(),
                )
                .await
        })
    };

    player_update_tasks.push(PlayerUpdateTask {
        runtime: livekit_runtime.clone(),
        task,
    });
}

fn participant_metadata_changed(
    trigger: Trigger<ParticipantMetadataChanged>,
    global_crdt_state: Res<GlobalCrdtState>,
    mut player_update_tasks: ResMut<PlayerUpdateTasks>,
    livekit_runtime: Res<LivekitRuntime>,
) {
    let ParticipantMetadataChanged { room, participant } = trigger.event();

    let meta = participant.metadata();
    if !meta.is_empty() {
        debug!(
            "Metadata of {} ({}) changed.",
            participant.sid(),
            participant.identity()
        );
        if let Some(address) = participant.identity().as_str().as_h160() {
            let room = *room;
            let sender = global_crdt_state.get_sender();
            let task = livekit_runtime.spawn(async move {
                sender
                    .send(
                        PlayerUpdate {
                            transport_id: room,
                            message: PlayerMessage::MetaData(meta),
                            address,
                        }
                        .into(),
                    )
                    .await
            });
            player_update_tasks.push(PlayerUpdateTask {
                runtime: livekit_runtime.clone(),
                task,
            });
        }
    }
}

fn stream_viewer_without_stream_image(
    mut commands: Commands,
    stream_viewers: Populated<(Entity, &StreamViewer), Without<StreamImage>>,
    stream_broadcasts: Query<&StreamImage, With<StreamBroadcast>>,
) {
    for (entity, stream_viewer) in stream_viewers.into_inner() {
        let Ok(stream_image) = stream_broadcasts.get(stream_viewer.get()) else {
            error!("Invalid StreamBroadcast relationship.");
            commands.send_event(AppExit::from_code(1));
            return;
        };

        commands.entity(entity).insert(stream_image.clone());
    }
}

#[expect(clippy::type_complexity, reason = "Queries are complex")]
fn non_stream_viewer_with_stream_image(
    mut commands: Commands,
    stream_viewers: Populated<
        Entity,
        (
            Without<StreamViewer>,
            Without<StreamBroadcast>,
            With<StreamImage>,
        ),
    >,
) {
    for entity in stream_viewers.into_inner() {
        commands.entity(entity).remove::<StreamImage>();
    }
}

fn someone_wants_to_watch_stream(
    trigger: Trigger<OnAdd, StreamBroadcast>,
    mut commands: Commands,
    participants: Query<(&LivekitParticipant, Option<&Publishing>), With<Streamer>>,
    audio_tracks: Query<(), With<Audio>>,
    video_tracks: Query<(), With<Video>>,
    mut images: ResMut<Assets<Image>>,
) {
    let entity = trigger.target();
    let Ok((participant, maybe_publishing)) = participants.get(entity) else {
        error!("StreamBroadcast on a non-Streamer participant.");
        commands.send_event(AppExit::from_code(1));
        return;
    };

    let mut image = Image::new_fill(
        bevy::render::render_resource::Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &palettes::basic::FUCHSIA.to_u8_array(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::all(),
    );
    image.texture_descriptor.usage = TextureUsages::COPY_DST | TextureUsages::TEXTURE_BINDING;

    debug!(
        "Streamer {} ({}) is now being watched.",
        participant.sid(),
        participant.identity()
    );
    commands
        .entity(entity)
        .insert(StreamImage(images.add(image)));

    if let Some(publishing) = maybe_publishing {
        if let Some(audio_track) = publishing
            .iter()
            .find(|published_track| audio_tracks.contains(*published_track))
        {
            commands.trigger_targets(SubscribeToTrack, audio_track);
        } else {
            debug!(
                "Participant {} ({}) is being watched but do not have any published audio track.",
                participant.sid(),
                participant.identity()
            );
        }
        if let Some(video_track) = publishing
            .iter()
            .find(|published_track| video_tracks.contains(*published_track))
        {
            commands.trigger_targets(SubscribeToTrack, video_track);
        } else {
            debug!(
                "Participant {} ({}) is being watched but do not have any published video track.",
                participant.sid(),
                participant.identity()
            );
        }
    }
}

fn noone_is_watching_stream(
    trigger: Trigger<OnRemove, StreamBroadcast>,
    mut commands: Commands,
    participants: Query<(&LivekitParticipant, Option<&Publishing>), With<Streamer>>,
    tracks: Query<&LivekitTrack>,
    livekit_runtime: Res<LivekitRuntime>,
) {
    let entity = trigger.target();
    let Ok((participant, maybe_publishing)) = participants.get(entity) else {
        error!("StreamBroadcast on a non-Streamer participant.");
        commands.send_event(AppExit::from_code(1));
        return;
    };
    debug!(
        "Streamer {} ({}) no longer being watched.",
        participant.sid(),
        participant.identity()
    );
    commands.entity(entity).try_remove::<StreamImage>();

    if let Some(publishing) = maybe_publishing {
        for livekit_track in tracks.iter_many(publishing.collection()) {
            let track = livekit_track.clone();
            livekit_runtime.spawn(async move {
                track.set_subscribed(false);
            });
        }
    }
}
