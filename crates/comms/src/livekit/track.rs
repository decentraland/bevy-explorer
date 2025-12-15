use bevy::{
    ecs::{component::HookContext, relationship::Relationship, world::DeferredWorld},
    prelude::*,
};
use livekit::{
    prelude::{Participant, RemoteTrackPublication},
    track::{TrackKind, TrackSource},
};
#[cfg(not(target_arch = "wasm32"))]
use tokio::task::JoinHandle;

use crate::{livekit::participant::LivekitParticipant, make_hooks};

#[derive(Component, Deref, DerefMut)]
pub struct LivekitTrack {
    track: RemoteTrackPublication,
}

#[derive(Component)]
#[relationship(relationship_target=Publishing)]
pub struct PublishedBy(Entity);

#[derive(Component)]
#[relationship_target(relationship=PublishedBy, linked_spawn)]
pub struct Publishing(Vec<Entity>);

#[derive(Component)]
#[component(on_add=Self::on_add)]
pub struct Subscribed;
make_hooks!(Subscribed, (Unsubscribed, Subscribing, Unsubscribing));

#[derive(Component)]
#[component(on_add=Self::on_add)]
pub struct Unsubscribed;
make_hooks!(Unsubscribed, (Subscribed, Subscribing, Unsubscribing));

#[derive(Component)]
#[component(on_add=Self::on_add)]
pub struct Subscribing(#[cfg(not(target_arch = "wasm32"))] JoinHandle<()>);
make_hooks!(Subscribing, (Subscribed, Unsubscribed, Unsubscribing));

#[derive(Component)]
#[component(on_add=Self::on_add)]
pub struct Unsubscribing(#[cfg(not(target_arch = "wasm32"))] JoinHandle<()>);
make_hooks!(Unsubscribing, (Subscribed, Unsubscribed, Subscribing));

#[derive(Component)]
pub struct Audio;

#[derive(Component)]
pub struct Video;

#[derive(Component)]
pub struct Microphone;

#[derive(Component)]
pub struct Camera;

#[derive(Event)]
pub struct TrackPublished {
    pub participant: Participant,
    pub track: RemoteTrackPublication,
}

#[derive(Event)]
pub struct TrackUnpublished {
    pub participant: Participant,
    pub track: RemoteTrackPublication,
}

#[derive(Event)]
pub struct TrackSubscribed {
    pub track: RemoteTrackPublication,
}

#[derive(Event)]
pub struct TrackUnsubscribed {
    pub track: RemoteTrackPublication,
}

pub(super) struct LivekitTrackPlugin;

impl Plugin for LivekitTrackPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(track_published);
        app.add_observer(track_unpublished);
        app.add_observer(track_subscribed);
        app.add_observer(track_unsubscribed);
    }
}

fn track_published(
    trigger: Trigger<TrackPublished>,
    mut commands: Commands,
    participants: Query<(Entity, &LivekitParticipant)>,
) {
    let TrackPublished { participant, track } = trigger.event();

    let Some(entity) = participants
        .iter()
        .find_map(|(entity, livekit_participant)| {
            if livekit_participant.sid() == participant.sid() {
                Some(entity)
            } else {
                None
            }
        })
    else {
        error!("No participant entity with sid {}.", participant.sid());
        commands.send_event(AppExit::from_code(1));
        return;
    };

    debug!(
        "{} ({}) published {:?} track {}.",
        participant.sid(),
        participant.identity(),
        track.kind(),
        track.sid(),
    );
    let mut entity_cmd = commands.spawn((
        LivekitTrack {
            track: track.clone(),
        },
        PublishedBy(entity),
        Unsubscribed,
    ));
    match track.kind() {
        TrackKind::Audio => {
            entity_cmd.insert(Audio);
        }
        TrackKind::Video => {
            entity_cmd.insert(Video);
        }
    }
    match track.source() {
        TrackSource::Microphone => {
            entity_cmd.insert(Microphone);
        }
        TrackSource::Camera => {
            entity_cmd.insert(Camera);
        }
        source => warn!("Track {} had {:?} source.", track.sid(), source),
    }
}

fn track_unpublished(
    trigger: Trigger<TrackUnpublished>,
    mut commands: Commands,
    tracks: Query<(Entity, &LivekitTrack, &PublishedBy)>,
    participants: Query<(Entity, &LivekitParticipant)>,
) {
    let TrackUnpublished { participant, track } = trigger.event();

    let Some(participant_entity) = participants
        .iter()
        .find_map(|(entity, livekit_participant)| {
            if livekit_participant.sid() == participant.sid() {
                Some(entity)
            } else {
                None
            }
        })
    else {
        error!("No participant entity with sid {}.", participant.sid());
        commands.send_event(AppExit::from_code(1));
        return;
    };

    let Some((entity, published_by)) =
        tracks
            .iter()
            .find_map(|(entity, livekit_track, published_by)| {
                if livekit_track.sid() == track.sid() {
                    Some((entity, published_by))
                } else {
                    None
                }
            })
    else {
        error!("No track entity with sid {}.", track.sid());
        commands.send_event(AppExit::from_code(1));
        return;
    };

    if published_by.get() != participant_entity {
        error!(
            "Unpublished track {} was not published by {}.",
            track.sid(),
            participant.sid()
        );
        commands.send_event(AppExit::from_code(1));
        return;
    }

    debug!(
        "{} ({}) unpublished {:?} track {}.",
        participant.sid(),
        participant.identity(),
        track.kind(),
        track.sid(),
    );
    commands.entity(entity).despawn();
}

fn track_subscribed(
    trigger: Trigger<TrackSubscribed>,
    mut commands: Commands,
    tracks: Query<(Entity, &LivekitTrack)>,
) {
    let TrackSubscribed { track } = trigger.event();

    let Some((entity, _)) = tracks
        .iter()
        .find(|(_, subscribing)| subscribing.sid() == track.sid())
    else {
        error!("No subscribing track with sid {}.", track.sid());
        commands.send_event(AppExit::from_code(1));
        return;
    };

    debug!("Subscribed to track {}.", track.sid());
    commands.entity(entity).insert(Subscribed);
}

fn track_unsubscribed(
    trigger: Trigger<TrackUnsubscribed>,
    mut commands: Commands,
    tracks: Query<(Entity, &LivekitTrack)>,
) {
    let TrackUnsubscribed { track } = trigger.event();

    let Some((entity, _)) = tracks
        .iter()
        .find(|(_, unsubscribing)| unsubscribing.sid() == track.sid())
    else {
        error!("No unsubscribing track with sid {}.", track.sid());
        commands.send_event(AppExit::from_code(1));
        return;
    };

    debug!("Unsubscribed to track {}.", track.sid());
    commands.entity(entity).insert(Unsubscribed);
}
