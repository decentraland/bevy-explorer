pub(super) mod plugin;

use std::marker::PhantomData;

use bevy::{
    ecs::{component::HookContext, world::DeferredWorld},
    platform::sync::Arc,
    prelude::*,
};
#[cfg(not(target_arch = "wasm32"))]
use livekit::{
    participant::Participant,
    prelude::{LocalParticipant, RemoteParticipant},
};

#[cfg(target_arch = "wasm32")]
use crate::livekit::web::{LocalParticipant, Participant, RemoteParticipant};
use crate::make_hooks;

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

#[derive(Component)]
#[relationship(relationship_target=StreamBroadcast)]
pub struct StreamViewer(Entity);

#[derive(Component)]
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
pub struct ParticipantConnectionQuality<C: Component> {
    participant: LivekitParticipant,
    room: Entity,
    phantom_data: PhantomData<C>,
}

impl<C: Component> ParticipantConnectionQuality<C> {
    #[expect(unused_variables, reason = "Parameter exists to help type inference")]
    pub fn new(participant: LivekitParticipant, room: Entity, connection_quality: C) -> Self {
        Self {
            participant,
            room,
            phantom_data: PhantomData,
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

pub mod connection_quality {
    use super::*;

    #[derive(Default, Component)]
    #[component(on_add=Self::on_add)]
    pub struct Excellent;
    make_hooks!(Excellent, (Good, Poor, Lost));

    #[derive(Default, Component)]
    #[component(on_add=Self::on_add)]
    pub struct Good;
    make_hooks!(Good, (Excellent, Poor, Lost));

    #[derive(Default, Component)]
    #[component(on_add=Self::on_add)]
    pub struct Poor;
    make_hooks!(Poor, (Excellent, Good, Lost));

    #[derive(Default, Component)]
    #[component(on_add=Self::on_add)]
    pub struct Lost;
    make_hooks!(Lost, (Excellent, Good, Poor));
}
