use std::marker::PhantomData;

use bevy::{
    ecs::{component::HookContext, relationship::Relationship, world::DeferredWorld},
    prelude::*,
};
#[cfg(not(target_arch = "wasm32"))]
use livekit::{
    participant::Participant,
    prelude::{LocalParticipant, RemoteParticipant},
};

#[cfg(target_arch = "wasm32")]
use crate::livekit::web::Participant;
use crate::{livekit::room::LivekitRoom, make_hooks};

#[derive(Clone, Component, Deref)]
pub struct LivekitParticipant {
    participant: Participant,
}

#[cfg(not(target_arch = "wasm32"))]
impl From<Participant> for LivekitParticipant {
    fn from(participant: Participant) -> Self {
        Self { participant }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<LocalParticipant> for LivekitParticipant {
    fn from(participant: LocalParticipant) -> Self {
        Self {
            participant: Participant::Local(participant),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
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

pub(super) struct ParticipantPlugin;

impl Plugin for ParticipantPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(participant_connected);
        app.add_observer(participant_disconnected);
        app.add_observer(participant_connection_quality_changed::<connection_quality::Excellent>);
        app.add_observer(participant_connection_quality_changed::<connection_quality::Good>);
        app.add_observer(participant_connection_quality_changed::<connection_quality::Poor>);
        app.add_observer(participant_connection_quality_changed::<connection_quality::Lost>);
    }
}

fn participant_connected(trigger: Trigger<ParticipantConnected>, mut commands: Commands) {
    let ParticipantConnected { participant, room } = trigger.event();
    debug!(
        "Participant '{}' ({}) connected to room {}.",
        participant.sid(),
        participant.identity(),
        room
    );

    #[cfg(not(target_arch = "wasm32"))]
    let is_local = matches!(participant.participant, Participant::Local(_));
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
    participants: Query<(Entity, &LivekitParticipant)>,
    rooms: Query<&HostingParticipants, With<LivekitRoom>>,
) {
    let ParticipantDisconnected { participant, room } = trigger.event();
    debug!(
        "Participant '{}' ({}) disconnected from room {}.",
        participant.sid(),
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
            if ecs_participant.identity() == participant.identity() {
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
