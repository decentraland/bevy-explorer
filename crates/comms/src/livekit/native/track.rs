use bevy::{
    ecs::{
        component::HookContext, relationship::Relationship, system::SystemParam,
        world::DeferredWorld,
    },
    platform::collections::HashMap,
    prelude::*,
};
use common::{structs::AudioDecoderError, util::AsH160};
use futures_lite::StreamExt;
use kira::sound::streaming::StreamingSoundData;
use livekit::{
    id::TrackSid,
    prelude::RemoteTrackPublication,
    track::{RemoteAudioTrack, RemoteTrack, RemoteVideoTrack, TrackKind},
};
use tokio::task::JoinHandle;

use crate::{
    global_crdt::{GlobalCrdtState, PlayerMessage, PlayerUpdate},
    livekit::{
        native::{
            participant::{Participant, PublishingTracks},
            LivekitKiraBridge, LivekitRuntime,
        },
        TransportedBy,
    },
};

pub struct TrackPlugin;

impl Plugin for TrackPlugin {
    fn build(&self, app: &mut bevy::app::App) {
        app.init_resource::<TrackMapper>();

        app.add_observer(request_subscription);
        app.add_observer(request_unsubscription);
        app.add_observer(subscription_received);
        app.add_observer(unsubscription_received);
        app.add_observer(abort_attached_audio);

        app.register_type::<PublishedBy>();
        app.register_type::<TrackMapper>();
    }
}

#[derive(SystemParam, Deref, DerefMut)]
#[expect(clippy::type_complexity, reason = "Queries are complex")]
pub struct Tracks<'w, 's> {
    commands: Commands<'w, 's>,
    track_mapper: ResMut<'w, TrackMapper>,
    #[deref]
    tracks: Query<
        'w,
        's,
        (
            NameOrEntity,
            &'static Track,
            AnyOf<(&'static Audio, &'static Video)>,
            &'static PublishedBy,
            &'static TransportedBy,
            Option<&'static Subscribed>,
        ),
    >,
    transports: Query<'w, 's, &'static LivekitRuntime>,
}

/// Maps between [`TrackSid`] and [`Entity`].
#[derive(Default, Reflect, Resource, Deref, DerefMut)]
#[reflect(Resource)]
struct TrackMapper(HashMap<String, Entity>);

#[derive(Component, Deref, DerefMut)]
pub struct Track(RemoteTrackPublication);

/// This is an Audio track
#[derive(Component)]
pub struct Audio;

/// This is an Video track
#[derive(Component)]
pub struct Video;

/// Marks that this track is subscribed.
#[derive(Component)]
#[component(on_insert = on_insert_subscribed)]
pub enum Subscribed {
    Audio(RemoteAudioTrack),
    Video(RemoteVideoTrack),
}

#[derive(Component)]
#[component(on_insert = on_insert_unsubscribed)]
pub struct Unsubscribed;

/// Marks that this track is being subscribed to.
#[derive(Component)]
#[component(on_insert = on_insert_subscribing)]
pub struct Subscribing;

/// Marks that this track is being unsubscribed from.
#[derive(Component)]
#[component(on_insert = on_insert_unsubscribing)]
pub struct Unsubscribing;

/// Handle to the task that consume an audio track.
#[derive(Component, Deref, DerefMut)]
pub struct AttachedAudio(JoinHandle<()>);

#[derive(Reflect, Component)]
#[reflect(Component)]
#[relationship(relationship_target = PublishingTracks)]
pub struct PublishedBy(Entity);

impl<'w, 's> Tracks<'w, 's> {
    pub fn track_published(
        &mut self,
        participant: Entity,
        transport: Entity,
        publication: RemoteTrackPublication,
    ) {
        debug!("{} published {}.", participant, publication.sid());
        let publication_sid = publication.sid();
        let publication_id = self
            .commands
            .spawn((
                Name::new(publication.sid().to_string()),
                Track(publication),
                Unsubscribed,
                PublishedBy(participant),
                TransportedBy(transport),
            ))
            .id();
        self.track_mapper
            .insert(publication_sid.to_string(), publication_id);
    }

    /// Removes a track.
    pub fn track_unpublished(&mut self, remote_track: RemoteTrackPublication) {
        let sid = remote_track.sid();

        if let Some(entity) = self.track_mapper.remove(sid.as_str()) {
            debug!("Track {} unpublished.", sid);
            self.commands.entity(entity).despawn();
        } else {
            error!("Track {} was not mapped.", sid);
        }
    }

    /// Attempt to subscribe to a track
    pub fn subscribe(&mut self, remote_track: RemoteTrackPublication) {
        let sid = remote_track.sid();

        if let Some(track_id) = self.track_mapper.get(sid.as_str()).copied() {
            debug!("Subscribing to track {}.", sid);
            self.commands.entity(track_id).insert(Subscribing);
        } else {
            error!("Track {} is not mapped.", sid);
        }
    }

    /// Subscribed to a track
    pub fn subscribed(&mut self, track: RemoteTrack, remote_track: RemoteTrackPublication) {
        let sid = remote_track.sid();

        if let Some(track_id) = self.track_mapper.get(sid.as_str()).copied() {
            debug!("Track {} subscribed.", sid);
            let subscription = match track {
                RemoteTrack::Audio(audio) => Subscribed::Audio(audio),
                RemoteTrack::Video(video) => Subscribed::Video(video),
            };
            self.commands.entity(track_id).insert(subscription);
        } else {
            error!("Track {} is not mapped.", sid);
        }
    }

    /// Unsubscribed from a track
    pub fn unsubscribed(&mut self, remote_track: RemoteTrackPublication) {
        let sid = remote_track.sid();
        self.unsubscribed_track_sid(sid);
    }

    /// Unsubscribed from a track
    pub fn unsubscribed_track_sid(&mut self, sid: TrackSid) {
        if let Some(track_id) = self.track_mapper.get(sid.as_str()).copied() {
            debug!("Track {} unsubscribed.", sid);
            self.commands.entity(track_id).insert(Unsubscribed);
        } else {
            error!("Track {} is not mapped.", sid);
        }
    }

    /// Creates a task to consume the audio frames from the [`RemoteAudioTrack`]
    /// and send it into the [`Sender`](tokio::sync::oneshot::Sender).
    pub fn attach_sender_to_audio_track(
        &mut self,
        publication: RemoteTrackPublication,
        sender: tokio::sync::oneshot::Sender<StreamingSoundData<AudioDecoderError>>,
    ) {
        let sid = publication.sid();
        let Some(entity) = self.track_mapper.get(sid.as_str()) else {
            error!("Track {} is not mapped.", sid);
            return;
        };

        let Ok((_, track, _, _, transported_by, maybe_subscribed)) = self.tracks.get(*entity)
        else {
            error!("Track {} was mapped but did not return from query.", sid);
            return;
        };

        let Ok(livekit_runtime) = self.transports.get(transported_by.get()) else {
            unreachable!("Relationship must be valid.");
        };

        let Some(subscribed) = maybe_subscribed else {
            error!("Track {} was not subscribed to.", sid);
            return;
        };

        let Subscribed::Audio(audio_track) = subscribed else {
            error!("Track {} was not an audio track.", sid);
            return;
        };

        let remote_track_publication = track.0.clone();
        let remote_track = audio_track.clone();
        let attached_audio = livekit_runtime.spawn(subscribe_remote_track_audio(
            remote_track,
            sender,
            remote_track_publication,
        ));

        self.commands
            .entity(*entity)
            .insert(AttachedAudio(attached_audio));
    }
}

/// On inserting [`Subscribing`], trigger a task to send the subscription
/// request.
fn request_subscription(
    trigger: Trigger<OnInsert, Subscribing>,
    tracks: Query<(&Track, &TransportedBy)>,
    transports: Query<&LivekitRuntime>,
) {
    let Ok((track, transported_by)) = tracks.get(trigger.target()) else {
        error!("Subscribing added to an entity that is not a Track.");
        return;
    };

    let Ok(transport) = transports.get(transported_by.get()) else {
        unreachable!("Relationship must be valid.");
    };

    let track = (*track).clone();
    transport.spawn(async move {
        track.set_subscribed(true);
    });
}

/// On inserting [`Unsubscribing`], trigger a task to send the unsubscription
/// request.
fn request_unsubscription(
    trigger: Trigger<OnInsert, Unsubscribing>,
    tracks: Query<(&Track, &TransportedBy)>,
    transports: Query<&LivekitRuntime>,
) {
    let Ok((track, transported_by)) = tracks.get(trigger.target()) else {
        error!("Unsubscribing added to an entity that is not a Track.");
        return;
    };

    let Ok(transport) = transports.get(transported_by.get()) else {
        unreachable!("Relationship must be valid.");
    };

    let track = (*track).clone();
    transport.spawn(async move {
        track.set_subscribed(false);
    });
}

/// Sends a [`PlayerUpdate`] informing that an audio track became available
fn subscription_received(
    trigger: Trigger<OnInsert, Subscribed>,
    tracks: Query<(&Track, &PublishedBy, &TransportedBy)>,
    transports: Query<&LivekitRuntime>,
    participants: Query<&Participant>,
    player_state: Res<GlobalCrdtState>,
) {
    let Ok((track, published_by, transported_by)) = tracks.get(trigger.target()) else {
        error!("Subscribing added to an entity that is not a Track.");
        return;
    };

    let Ok(participant) = participants.get(published_by.get()) else {
        unreachable!("Relationship must be valid.");
    };
    let Ok(transport) = transports.get(transported_by.get()) else {
        unreachable!("Relationship must be valid.");
    };

    if let Some(address) = participant.identity().as_str().as_h160() {
        let sender = player_state.get_sender();
        if matches!(track.kind(), TrackKind::Audio) {
            let transport_id = transported_by.get();
            transport.spawn(async move {
                let _ = sender
                    .send(PlayerUpdate {
                        transport_id,
                        message: PlayerMessage::AudioStreamAvailable {
                            transport: transport_id,
                        },
                        address,
                    })
                    .await;
            });
        }
    }
}

/// Send a [`PlayerUpdate`] informing that the audio track has been
/// unsubscribed.
fn unsubscription_received(
    trigger: Trigger<OnInsert, Unsubscribed>,
    tracks: Query<(&Track, &PublishedBy, &TransportedBy)>,
    transports: Query<&LivekitRuntime>,
    participants: Query<&Participant>,
    player_state: Res<GlobalCrdtState>,
) {
    let Ok((track, published_by, transported_by)) = tracks.get(trigger.target()) else {
        error!("Subscribing added to an entity that is not a Track.");
        return;
    };

    let Ok(participant) = participants.get(published_by.get()) else {
        unreachable!("Relationship must be valid.");
    };
    let Ok(transport) = transports.get(transported_by.get()) else {
        unreachable!("Relationship must be valid.");
    };

    if let Some(address) = participant.identity().as_str().as_h160() {
        let sender = player_state.get_sender();
        if matches!(track.kind(), TrackKind::Audio) {
            let transport_id = transported_by.get();
            transport.spawn(async move {
                let _ = sender
                    .send(PlayerUpdate {
                        transport_id,
                        message: PlayerMessage::AudioStreamUnavailable {
                            transport: transport_id,
                        },
                        address,
                    })
                    .await;
            });
        }
    }
}

/// Aborts a dropped [`AttachedAudio`] stream.
fn abort_attached_audio(
    trigger: Trigger<OnReplace, AttachedAudio>,
    tracks: Query<(&Track, &AttachedAudio)>,
) {
    let Ok((track, attached_audio)) = tracks.get(trigger.target()) else {
        unreachable!("Track entity should be well-formed.");
    };

    debug!("Aborting attached audio task of track {}.", track.sid());
    attached_audio.abort();
}

/// Hook that runs when [`Unsubscribing`] is inserted on an entity.
///
/// Removes [`Subscribed`] and [`Unsubscribed`].
fn on_insert_subscribed(mut world: DeferredWorld, hook_context: HookContext) {
    world
        .commands()
        .entity(hook_context.entity)
        .remove::<(Subscribing, Unsubscribing, Unsubscribed)>();
}

/// Hook that runs when [`Unsubscribed`] is inserted on an entity.
///
/// Removes [`Subscribed`] and [`Unsubscribing`].
fn on_insert_unsubscribed(mut world: DeferredWorld, hook_context: HookContext) {
    world.commands().entity(hook_context.entity).remove::<(
        Subscribed,
        Subscribing,
        Unsubscribing,
        AttachedAudio,
    )>();
}

/// Hook that runs when [`Unsubscribing`] is inserted on an entity.
///
/// Removes [`Subscribed`] and [`Unsubscribed`].
fn on_insert_subscribing(mut world: DeferredWorld, hook_context: HookContext) {
    world
        .commands()
        .entity(hook_context.entity)
        .remove::<(Subscribed, Unsubscribed, Unsubscribing)>();
}

/// Hook that runs when [`Unsubscribing`] is inserted on an entity.
///
/// Removes [`Subscribed`] and [`Unsubscribed`].
fn on_insert_unsubscribing(mut world: DeferredWorld, hook_context: HookContext) {
    world
        .commands()
        .entity(hook_context.entity)
        .remove::<(Subscribed, Subscribing, Unsubscribed)>();
}

async fn subscribe_remote_track_audio(
    audio: RemoteAudioTrack,
    channel: tokio::sync::oneshot::Sender<StreamingSoundData<AudioDecoderError>>,
    publication: RemoteTrackPublication,
) {
    let mut stream =
        livekit::webrtc::audio_stream::native::NativeAudioStream::new(audio.rtc_track(), 48_000, 1);

    // get first frame to set sample rate
    let Some(frame) = stream.next().await else {
        warn!("dropped audio track without samples");
        return;
    };

    let (frame_sender, frame_receiver) = tokio::sync::mpsc::channel(1000);

    let bridge = LivekitKiraBridge {
        started: false,
        sample_rate: frame.sample_rate,
        receiver: frame_receiver,
    };

    debug!("recced with {} / {}", frame.sample_rate, frame.num_channels);

    let sound_data = kira::sound::streaming::StreamingSoundData::from_decoder(bridge);

    let res = channel.send(sound_data);

    if res.is_err() {
        warn!("failed to send subscribed audio data");
        publication.set_subscribed(false);
        return;
    }

    while let Some(frame) = stream.next().await {
        match frame_sender.try_send(frame) {
            Ok(()) => (),
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                warn!("livekit audio receiver buffer full, dropping frame");
                return;
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                warn!("livekit audio receiver dropped, exiting task");
                return;
            }
        }
    }

    warn!("track ended, exiting task");
}
