use bevy::{ecs::relationship::Relationship, prelude::*};
#[cfg(not(target_arch = "wasm32"))]
use livekit::{
    participant::Participant as LivekitParticipant,
    prelude::{LocalParticipant, RemoteParticipant},
};

use crate::livekit::room::LivekitRoom;
#[cfg(target_arch = "wasm32")]
use crate::livekit::web::Participant as LivekitParticipant;

#[derive(Clone, Component, Deref)]
pub struct Participant {
    participant: LivekitParticipant,
}

#[cfg(not(target_arch = "wasm32"))]
impl From<LocalParticipant> for Participant {
    fn from(participant: LocalParticipant) -> Self {
        Self {
            participant: LivekitParticipant::Local(participant),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<RemoteParticipant> for Participant {
    fn from(participant: RemoteParticipant) -> Self {
        Self {
            participant: LivekitParticipant::Remote(participant),
        }
    }
}

/// Marks a participant as being local
#[derive(Component)]
pub struct Local;

/// Marks a participant as being a streamer.
/// Streamers have their identity ending with `-streamer`.
#[derive(Component)]
pub struct Streamer;

#[derive(Component)]
#[relationship(relationship_target=HostingParticipants)]
pub struct HostedBy(Entity);

#[derive(Component)]
#[relationship_target(relationship=HostedBy)]
pub struct HostingParticipants(Vec<Entity>);

#[derive(Event)]
pub struct ParticipantConnected {
    pub participant: Participant,
    pub room: Entity,
}

#[derive(Event)]
pub struct ParticipantDisconnected {
    pub participant: Participant,
    pub room: Entity,
}

pub(super) struct ParticipantPlugin;

impl Plugin for ParticipantPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(participant_connected);
        app.add_observer(participant_disconnected);
    }
}

fn participant_connected(trigger: Trigger<ParticipantConnected>, mut commands: Commands) {
    let ParticipantConnected { participant, room } = trigger.event();
    debug!(
        "Participant '{}' ({}) connected to room {}.",
        participant.name(),
        participant.identity(),
        room
    );

    #[cfg(not(target_arch = "wasm32"))]
    let is_local = matches!(participant.participant, LivekitParticipant::Local(_));
    #[cfg(target_arch = "wasm32")]
    let is_local = false;

    if is_local {
        commands.spawn((
            participant.clone(),
            <HostedBy as Relationship>::from(*room),
            Local,
        ));
    } else if participant.identity().as_str().ends_with("-streamer") {
        commands.spawn((
            participant.clone(),
            <HostedBy as Relationship>::from(*room),
            Streamer,
        ));
    } else {
        commands.spawn((participant.clone(), <HostedBy as Relationship>::from(*room)));
    }
}

fn participant_disconnected(
    trigger: Trigger<ParticipantDisconnected>,
    mut commands: Commands,
    participants: Query<(Entity, &Participant)>,
    rooms: Query<&HostingParticipants, With<LivekitRoom>>,
) {
    let ParticipantDisconnected { participant, room } = trigger.event();
    debug!(
        "Participant '{}' ({}) disconnected from room {}.",
        participant.name(),
        participant.identity(),
        room
    );

    let Ok(hosting_participants) = rooms.get(*room) else {
        error!("Room given to ParticipantDisconnected was invalid.");
        commands.send_event(AppExit::from_code(1));
        return;
    };

    let Some(entity) = participants
        .iter_many(hosting_participants.collection())
        .find_map(|(entity, ecs_participant)| {
            if ecs_participant.identity() == participant.identity() {
                Some(entity)
            } else {
                None
            }
        })
    else {
        error!(
            "No entity referent to '{}' ({}).",
            participant.name(),
            participant.identity()
        );
        return;
    };

    commands.entity(entity).despawn();
}
