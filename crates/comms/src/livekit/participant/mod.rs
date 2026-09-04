pub(super) mod plugin;

use bevy::{platform::sync::Arc, prelude::*};
#[cfg(not(target_arch = "wasm32"))]
use livekit::{
    participant::{ConnectionQuality as LivekitConnectionQuality, Participant},
    prelude::{LocalParticipant, RemoteParticipant},
};

#[cfg(target_arch = "wasm32")]
use crate::livekit::web::{
    ConnectionQuality as LivekitConnectionQuality, LocalParticipant, Participant, RemoteParticipant,
};

#[derive(Clone, Component, Deref)]
pub struct LivekitParticipant {
    participant: Participant,
}

impl From<Participant> for LivekitParticipant {
    fn from(participant: Participant) -> Self {
        Self { participant }
    }
}

impl From<LocalParticipant> for LivekitParticipant {
    fn from(participant: LocalParticipant) -> Self {
        Self {
            participant: Participant::Local(participant),
        }
    }
}

impl From<RemoteParticipant> for LivekitParticipant {
    fn from(participant: RemoteParticipant) -> Self {
        Self {
            participant: Participant::Remote(participant),
        }
    }
}

/// Marks a participant as being local
#[derive(Component)]
pub struct Local;

#[derive(Component)]
#[relationship(relationship_target=HostingParticipants)]
pub struct HostedBy(Entity);

#[derive(Component)]
#[relationship_target(relationship=HostedBy, linked_spawn)]
pub struct HostingParticipants(Vec<Entity>);

#[derive(Event)]
pub struct ParticipantConnected {
    pub participant: LivekitParticipant,
    pub room: Entity,
}

#[derive(Event)]
pub struct ParticipantDisconnected {
    pub participant: LivekitParticipant,
    pub room: Entity,
}

#[derive(Event)]
pub struct ParticipantConnectionQuality {
    participant: LivekitParticipant,
    room: Entity,
    connection_quality: LivekitConnectionQuality,
}

impl ParticipantConnectionQuality {
    pub fn new(
        participant: LivekitParticipant,
        room: Entity,
        connection_quality: LivekitConnectionQuality,
    ) -> Self {
        Self {
            participant,
            room,
            connection_quality,
        }
    }
}

#[derive(Event)]
pub struct ParticipantPayload {
    pub room: Entity,
    pub participant: LivekitParticipant,
    pub payload: Arc<Vec<u8>>,
}

#[derive(Event)]
pub struct ParticipantMetadataChanged {
    pub room: Entity,
    pub participant: LivekitParticipant,
}

#[derive(Component)]
pub struct ActiveSpeaker;

#[derive(Event)]
pub struct ActiveSpeakersChanged {
    pub speakers: Vec<Participant>,
}
