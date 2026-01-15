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

/// Marks a participant as being a streamer.
/// Streamers have their identity ending with `-streamer`.
#[derive(Component)]
pub struct Streamer;

#[derive(Component)]
#[relationship(relationship_target=HostingParticipants)]
pub struct HostedBy(Entity);

#[derive(Component)]
#[relationship_target(relationship=HostedBy, linked_spawn)]
pub struct HostingParticipants(Vec<Entity>);

#[derive(Debug, Component)]
#[relationship(relationship_target=StreamBroadcast)]
pub struct StreamViewer(Entity);

#[derive(Debug, Component)]
#[relationship_target(relationship=StreamViewer)]
pub struct StreamBroadcast(Vec<Entity>);

#[derive(Clone, Component, Deref)]
pub struct StreamImage(Handle<Image>);

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
    connection_quality: ConnectionQuality,
}

impl ParticipantConnectionQuality {
    pub fn new<C: Into<ConnectionQuality>>(
        participant: LivekitParticipant,
        room: Entity,
        connection_quality: C,
    ) -> Self {
        Self {
            participant,
            room,
            connection_quality: connection_quality.into(),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub enum ConnectionQuality {
    Excellent,
    Good,
    Poor,
    Lost,
}

impl From<LivekitConnectionQuality> for ConnectionQuality {
    fn from(value: LivekitConnectionQuality) -> Self {
        match value {
            LivekitConnectionQuality::Excellent => Self::Excellent,
            LivekitConnectionQuality::Good => Self::Good,
            LivekitConnectionQuality::Poor => Self::Poor,
            LivekitConnectionQuality::Lost => Self::Lost,
        }
    }
}
