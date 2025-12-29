use bevy::{ecs::relationship::Relationship, prelude::*};
use common::util::AsH160;
use dcl_component::proto_components::kernel::comms::rfc4;
#[cfg(not(target_arch = "wasm32"))]
use livekit::prelude::Participant;
use prost::Message;

use crate::{
    global_crdt::{GlobalCrdtState, PlayerMessage, PlayerUpdate},
    livekit::{
        participant::{
            connection_quality, HostedBy, HostingParticipants, LivekitParticipant, Local,
            ParticipantConnected, ParticipantConnectionQuality, ParticipantDisconnected,
            ParticipantMetadataChanged, ParticipantPayload, Streamer, TransmittingTo,
        },
        plugin::{PlayerUpdateTask, PlayerUpdateTasks},
        room::LivekitRoom,
        track::{Publishing, UnsubscribeToTrack},
        LivekitRuntime,
    },
};

pub struct LivekitParticipantPlugin;

impl Plugin for LivekitParticipantPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(participant_connected);
        app.add_observer(participant_disconnected);
        app.add_observer(participant_connection_quality_changed::<connection_quality::Excellent>);
        app.add_observer(participant_connection_quality_changed::<connection_quality::Good>);
        app.add_observer(participant_connection_quality_changed::<connection_quality::Poor>);
        app.add_observer(participant_connection_quality_changed::<connection_quality::Lost>);
        app.add_observer(participant_payload);
        app.add_observer(participant_metadata_changed);
        app.add_observer(streamer_has_no_watchers);
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

    #[cfg(not(target_arch = "wasm32"))]
    let is_local = matches!(participant.participant, Participant::Local(_));
    #[cfg(target_arch = "wasm32")]
    let is_local = false;

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

fn participant_connection_quality_changed<C: Component + Default>(
    trigger: Trigger<ParticipantConnectionQuality<C>>,
    mut commands: Commands,
    participants: Query<(Entity, &LivekitParticipant)>,
    rooms: Query<&HostingParticipants, With<LivekitRoom>>,
) {
    let ParticipantConnectionQuality {
        participant, room, ..
    } = trigger.event();
    debug!(
        "Participant '{}' ({}) connection quality with {room} changed to {}.",
        participant.sid(),
        participant.identity(),
        disqualified::ShortName::of::<C>(),
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

    commands.entity(entity).insert(C::default());
}

fn participant_payload(
    trigger: Trigger<ParticipantPayload>,
    mut commands: Commands,
    rooms: Query<&LivekitRuntime, With<LivekitRoom>>,
    global_crdt_state: Res<GlobalCrdtState>,
    mut player_update_tasks: ResMut<PlayerUpdateTasks>,
) {
    let ParticipantPayload {
        room: room_entity,
        participant,
        payload,
    } = trigger.event();

    let Some(address) = participant.identity().as_str().as_h160() else {
        debug!(
            "Payload for non-player participant {} ({}) is ignored.",
            participant.sid(),
            participant.identity()
        );
        return;
    };
    let Ok(runtime) = rooms.get(*room_entity) else {
        error!("Room {room_entity} does not have a runtime.");
        commands.send_event(AppExit::from_code(1));
        return;
    };

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

    trace!(
        "[{}] received [{}] packet {message:?} from {address}",
        room_entity,
        packet.protocol_version
    );

    let room = *room_entity;
    let sender = global_crdt_state.get_sender();
    let task = runtime.spawn(async move {
        sender
            .send(PlayerUpdate {
                transport_id: room,
                message: PlayerMessage::PlayerData(message),
                address,
            })
            .await
    });
    player_update_tasks.push(PlayerUpdateTask {
        runtime: runtime.clone(),
        task,
    });
}

fn participant_metadata_changed(
    trigger: Trigger<ParticipantMetadataChanged>,
    mut commands: Commands,
    rooms: Query<&LivekitRuntime, With<LivekitRoom>>,
    global_crdt_state: Res<GlobalCrdtState>,
    mut player_update_tasks: ResMut<PlayerUpdateTasks>,
) {
    let ParticipantMetadataChanged { room, participant } = trigger.event();

    let Ok(runtime) = rooms.get(*room) else {
        error!("Room {room} does not have a runtime.");
        commands.send_event(AppExit::from_code(1));
        return;
    };

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
            let task = runtime.spawn(async move {
                sender
                    .send(PlayerUpdate {
                        transport_id: room,
                        message: PlayerMessage::MetaData(meta),
                        address,
                    })
                    .await
            });
            player_update_tasks.push(PlayerUpdateTask {
                runtime: runtime.clone(),
                task,
            });
        }
    }
}

#[expect(clippy::type_complexity, reason = "Queries are complex")]
fn streamer_has_no_watchers(
    trigger: Trigger<OnRemove, TransmittingTo>,
    mut commands: Commands,
    rooms: Query<&LivekitRuntime, With<LivekitRoom>>,
    participants: Populated<(&HostedBy, &Publishing), (With<Streamer>, Without<TransmittingTo>)>,
) {
    let entity = trigger.target();
    let Ok((hosted_by, publishing)) = participants.get(entity) else {
        error!("An entity that is not a participant had TransmittingTo.");
        commands.send_event(AppExit::from_code(1));
        return;
    };
    let Ok(runtime) = rooms.get(hosted_by.get()) else {
        error!("HostedBy relationship was broken.");
        commands.send_event(AppExit::from_code(1));
        return;
    };

    for track in publishing.iter() {
        commands.trigger_targets(
            UnsubscribeToTrack {
                runtime: runtime.clone(),
            },
            track,
        );
    }
}
