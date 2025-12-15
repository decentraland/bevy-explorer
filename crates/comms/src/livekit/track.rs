use bevy::{ecs::relationship::Relationship, prelude::*};
use livekit::{
    prelude::{Participant, RemoteTrackPublication},
    track::TrackKind,
};

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
pub struct Audio;

#[derive(Component)]
pub struct Video;

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

pub(super) struct LivekitTrackPlugin;

impl Plugin for LivekitTrackPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(track_published);
        app.add_observer(track_unpublished);
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
        "Published {:?} track {} of {} ({}).",
        track.kind(),
        track.sid(),
        participant.sid(),
        participant.identity()
    );
    let mut entity_cmd = commands.spawn((
        LivekitTrack {
            track: track.clone(),
        },
        PublishedBy(entity),
    ));
    match track.kind() {
        TrackKind::Audio => {
            entity_cmd.insert(Audio);
        }
        TrackKind::Video => {
            entity_cmd.insert(Video);
        }
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
        "Unpublished {:?} track {} of {} ({}).",
        track.kind(),
        track.sid(),
        participant.sid(),
        participant.identity()
    );
    commands.entity(entity).despawn();
}
