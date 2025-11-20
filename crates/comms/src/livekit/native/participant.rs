use bevy::{ecs::system::SystemParam, platform::collections::HashMap, prelude::*};
use livekit::participant::RemoteParticipant;

use crate::livekit::{native::track::PublishedBy, TransportedBy};

pub struct ParticipantPlugin;

impl Plugin for ParticipantPlugin {
    fn build(&self, app: &mut bevy::app::App) {
        app.init_resource::<ParticipantMapper>();

        app.register_type::<PublishingTracks>();
        app.register_type::<ParticipantMapper>();
    }
}

#[derive(SystemParam)]
pub struct Participants<'w, 's> {
    commands: Commands<'w, 's>,
    participant_mapper: ResMut<'w, ParticipantMapper>,
    participants: Query<'w, 's, (NameOrEntity, &'static Participant)>,
}

/// Maps between [`ParticipantIdentity`](livekit::room::id::ParticipantIdentity)/[`ParticipantSuid`](livekit::room::id::ParticipantSid)
/// pair and [`Entity`].
#[derive(Default, Reflect, Resource, Deref, DerefMut)]
#[reflect(Resource)]
struct ParticipantMapper(HashMap<(String, String), Entity>);

/// A participant in a [`LivekitRoom`](crate::livekit::native::LivekitRoom).
#[derive(Component, Deref, DerefMut)]
pub struct Participant(RemoteParticipant);

/// Tracks published by this [`Participant`] that will be subscribed
/// locally.
#[derive(Reflect, Component)]
#[reflect(Component)]
#[relationship_target(relationship = PublishedBy)]
pub struct PublishingTracks(Vec<Entity>);

impl<'w, 's> Participants<'w, 's> {
    /// Create an entity for a new [`RemoteParticipant`]
    pub fn new_participant(
        &mut self,
        transport: Entity,
        remote_participant: RemoteParticipant,
    ) -> Entity {
        let identity = remote_participant.identity();
        let sid = remote_participant.sid();

        if let Some(participant) = self
            .participant_mapper
            .get(&(identity.to_string(), sid.to_string()))
        {
            error!("Participant {} ({}) is already mapped.", identity, sid);
            *participant
        } else {
            debug!("Participant {} ({}) connected.", identity, sid);
            let participant = self
                .commands
                .spawn((
                    Name::new(format!("{}:{}", identity, sid)),
                    Participant(remote_participant),
                    TransportedBy(transport),
                ))
                .id();

            self.participant_mapper
                .insert((identity.to_string(), sid.to_string()), participant);

            participant
        }
    }

    /// Unmaps a disconnecting [`RemoteParticipant`].
    pub fn participant_disconnected(&mut self, remote_participant: RemoteParticipant) {
        let identity = remote_participant.identity();
        let sid = remote_participant.sid();

        if let Some(participant) = self
            .participant_mapper
            .remove(&(identity.to_string(), sid.to_string()))
        {
            debug!(
                "Participant {} ({}) disconnected.",
                remote_participant.identity(),
                remote_participant.sid()
            );
            self.commands.entity(participant).despawn();
        } else {
            error!(
                "Participant {} ({}) was not mapped.",
                remote_participant.identity(),
                remote_participant.sid()
            );
        }
    }

    /// Get the [`Entity`] of a [`RemoteParticipant`]
    pub fn get(&self, remote_participant: &RemoteParticipant) -> Option<Entity> {
        let identity = remote_participant.identity();
        let sid = remote_participant.sid();

        self.participant_mapper
            .get(&(identity.to_string(), sid.to_string()))
            .copied()
    }
}
