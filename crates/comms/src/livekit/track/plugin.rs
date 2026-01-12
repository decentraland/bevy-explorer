use bevy::{ecs::relationship::Relationship, prelude::*, render::render_resource::Extent3d};
use common::util::AsH160;
#[cfg(not(target_arch = "wasm32"))]
use {
    livekit::track::{RemoteTrack, TrackKind, TrackSource},
    livekit::webrtc::prelude::VideoBuffer,
    tokio::sync::{mpsc, oneshot},
};

#[cfg(target_arch = "wasm32")]
use crate::livekit::web::{TrackKind, TrackSource};
#[cfg(not(target_arch = "wasm32"))]
use crate::livekit::{
    kira_bridge::kira_thread,
    livekit_video_bridge::{livekit_video_thread, I420BufferExt},
    participant::StreamImage,
    track::{LivekitTrackTask, OpenAudioSender, VideoFrameReceiver},
};
use crate::{
    global_crdt::{GlobalCrdtState, PlayerMessage, PlayerUpdate},
    livekit::{
        participant::{HostedBy, LivekitParticipant, StreamBroadcast},
        plugin::{PlayerUpdateTask, PlayerUpdateTasks},
        track::{
            Audio, Camera, LivekitTrack, Microphone, PublishedBy, SubscribeToAudioTrack,
            SubscribeToTrack, Subscribed, Subscribing, TrackPublished, TrackSubscribed,
            TrackUnpublished, TrackUnsubscribed, UnsubscribeToTrack, Unsubscribed, Unsubscribing,
            Video,
        },
        LivekitRuntime,
    },
};

pub struct LivekitTrackPlugin;

impl Plugin for LivekitTrackPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(track_published);
        app.add_observer(track_unpublished);
        app.add_observer(track_subscribed);
        app.add_observer(track_unsubscribed);
        app.add_observer(subscribe_to_audio_track);
        app.add_observer(subscribe_to_track);
        app.add_observer(unsubscribe_to_track);

        #[cfg(not(target_arch = "wasm32"))]
        app.add_systems(
            Update,
            (subscribed_audio_track_with_open_sender, receive_video_frame),
        );
        #[cfg(not(target_arch = "wasm32"))]
        app.add_observer(video_track_is_now_subscribed);
        #[cfg(not(target_arch = "wasm32"))]
        app.add_observer(track_of_watched_streamer_published::<Video>);
        #[cfg(not(target_arch = "wasm32"))]
        app.add_observer(track_of_watched_streamer_published::<Audio>);
    }
}

fn track_published(
    trigger: Trigger<TrackPublished>,
    mut commands: Commands,
    participants: Query<(Entity, &LivekitParticipant, &HostedBy)>,
    player_state: Res<GlobalCrdtState>,
    mut player_update_tasks: ResMut<PlayerUpdateTasks>,
    livekit_runtime: Res<LivekitRuntime>,
) {
    let TrackPublished { participant, track } = trigger.event();

    let Some((entity, _, hosted_by)) = participants
        .iter()
        .find(|(_, livekit_participant, _)| livekit_participant.sid() == participant.sid())
    else {
        error!("No participant entity with sid {}.", participant.sid());
        commands.send_event(AppExit::from_code(1));
        return;
    };

    let room_entity = hosted_by.get();

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

    let maybe_address = participant.identity().as_str().as_h160();
    if maybe_address.is_some() && track.kind() == TrackKind::Audio {
        #[expect(
            clippy::unnecessary_unwrap,
            reason = "No let chains in current version."
        )]
        let address = maybe_address.unwrap();

        let sender = player_state.get_sender();
        let task = livekit_runtime.spawn(async move {
            sender
                .send(
                    PlayerUpdate {
                        transport_id: room_entity,
                        message: PlayerMessage::AudioStreamAvailable {
                            transport: room_entity,
                        },
                        address,
                    }
                    .into(),
                )
                .await
        });
        player_update_tasks.push(PlayerUpdateTask {
            runtime: livekit_runtime.clone(),
            task,
        });
    }
}

fn track_unpublished(
    trigger: Trigger<TrackUnpublished>,
    mut commands: Commands,
    tracks: Query<(Entity, &LivekitTrack, &PublishedBy)>,
    participants: Query<(Entity, &LivekitParticipant, &HostedBy)>,
    player_state: Res<GlobalCrdtState>,
    mut player_update_tasks: ResMut<PlayerUpdateTasks>,
    livekit_runtime: Res<LivekitRuntime>,
) {
    let TrackUnpublished { participant, track } = trigger.event();

    let Some((participant_entity, _, hosted_by)) = participants
        .iter()
        .find(|(_, livekit_participant, _)| livekit_participant.sid() == participant.sid())
    else {
        error!("No participant entity with sid {}.", participant.sid());
        commands.send_event(AppExit::from_code(1));
        return;
    };
    let room_entity = hosted_by.get();

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

    let maybe_address = participant.identity().as_str().as_h160();
    if maybe_address.is_some() && track.kind() == TrackKind::Audio {
        #[expect(
            clippy::unnecessary_unwrap,
            reason = "No let chains in current version."
        )]
        let address = maybe_address.unwrap();

        let sender = player_state.get_sender();
        let task = livekit_runtime.spawn(async move {
            sender
                .send(
                    PlayerUpdate {
                        transport_id: room_entity,
                        message: PlayerMessage::AudioStreamUnavailable {
                            transport: room_entity,
                        },
                        address,
                    }
                    .into(),
                )
                .await
        });

        player_update_tasks.push(PlayerUpdateTask {
            runtime: livekit_runtime.clone(),
            task,
        });
    }
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

fn subscribe_to_audio_track(
    mut trigger: Trigger<SubscribeToAudioTrack>,
    mut commands: Commands,
    tracks: Query<&LivekitTrack, With<Audio>>,
    livekit_runtime: Res<LivekitRuntime>,
) {
    let entity = trigger.target();
    let SubscribeToAudioTrack {
        #[cfg(not(target_arch = "wasm32"))]
        sender,
    } = trigger.event_mut();

    if entity == Entity::PLACEHOLDER {
        error!(
            "SubscribeToAudioTrack is an entity event. Call it with 'Commands::trigger_targets'."
        );
        return;
    }
    let Ok(track) = tracks.get(entity) else {
        error!("Can't subscribe to {} because it is not a track.", entity);
        return;
    };

    debug!("Subscribing to audio track {}", track.sid());
    let track = track.clone();
    #[cfg(not(target_arch = "wasm32"))]
    let snatcher_sender = {
        let (mut snatcher_sender, _) = oneshot::channel();
        std::mem::swap(&mut snatcher_sender, sender);
        snatcher_sender
    };

    let task = livekit_runtime.spawn(async move {
        track.set_subscribed(true);
    });
    commands.entity(entity).insert((
        Subscribing { task },
        #[cfg(not(target_arch = "wasm32"))]
        OpenAudioSender {
            sender: snatcher_sender,
        },
    ));
}

fn subscribe_to_track(
    trigger: Trigger<SubscribeToTrack>,
    mut commands: Commands,
    tracks: Query<&LivekitTrack, With<Video>>,
    livekit_runtime: Res<LivekitRuntime>,
) {
    let entity = trigger.target();

    if entity == Entity::PLACEHOLDER {
        error!(
            "SubscribeToVideoTrack is an entity event. Call it with 'Commands::trigger_targets'."
        );
        return;
    }
    let Ok(track) = tracks.get(entity) else {
        error!("Can't subscribe to {} because it is not a track.", entity);
        return;
    };

    #[cfg(not(target_arch = "wasm32"))]
    {
        let track = track.clone();

        debug!("Subscribing to video track {}", track.sid());
        let task = livekit_runtime.spawn(async move {
            track.set_subscribed(true);
        });
        commands.entity(entity).insert(Subscribing { task });
    }
}

fn unsubscribe_to_track(
    trigger: Trigger<UnsubscribeToTrack>,
    mut commands: Commands,
    tracks: Query<&LivekitTrack>,
    livekit_runtime: Res<LivekitRuntime>,
) {
    let entity = trigger.target();

    if entity == Entity::PLACEHOLDER {
        error!("UnsubscribeToTrack is an entity event. Call it with 'Commands::trigger_targets'.");
        return;
    }
    let Ok(track) = tracks.get(entity) else {
        error!("Can't unsubscribe to {} because it is not a track.", entity);
        return;
    };

    let track = track.clone();

    debug!("Unsubscribing to track {}", track.sid());
    let task = livekit_runtime.spawn(async move {
        track.set_subscribed(false);
    });
    commands.entity(entity).insert(Unsubscribing { task });
}

#[cfg(not(target_arch = "wasm32"))]
#[expect(clippy::type_complexity, reason = "Queries are complex")]
fn subscribed_audio_track_with_open_sender(
    mut commands: Commands,
    mut tracks: Populated<
        (Entity, &LivekitTrack, &mut OpenAudioSender),
        (With<Audio>, With<Subscribed>),
    >,
    livekit_runtime: Res<LivekitRuntime>,
) {
    for (entity, track, mut sender) in tracks.iter_mut() {
        let runtime = livekit_runtime.clone();
        let publication = track.track.clone();

        let Some(RemoteTrack::Audio(audio)) = track.track() else {
            error!("A subscribed audio track did not have a audio RemoteTrack.");
            commands.send_event(AppExit::from_code(1));
            return;
        };

        #[cfg(not(target_arch = "wasm32"))]
        {
            let (mut snatcher_sender, _) = oneshot::channel();
            std::mem::swap(&mut snatcher_sender, &mut sender.sender);

            let handle = runtime.spawn(kira_thread(audio, publication, snatcher_sender));
            commands
                .entity(entity)
                .insert(LivekitTrackTask(handle))
                .remove::<OpenAudioSender>();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn video_track_is_now_subscribed(
    trigger: Trigger<OnAdd, Subscribed>,
    mut commands: Commands,
    tracks: Query<&LivekitTrack, (With<Video>, With<Subscribed>)>,
    livekit_runtime: Res<LivekitRuntime>,
) {
    use crate::livekit::track::VideoFrameReceiver;

    let entity = trigger.target();
    let Ok(track) = tracks.get(entity) else {
        error!("Subscribed track was not a video.");
        commands.send_event(AppExit::from_code(1));
        return;
    };

    let runtime = livekit_runtime.clone();
    let publication = track.track.clone();

    let Some(RemoteTrack::Video(video)) = track.track() else {
        error!("A subscribed video track did not have a video RemoteTrack.");
        commands.send_event(AppExit::from_code(1));
        return;
    };

    let (sender, receiver) = mpsc::channel(60);
    let handle = runtime.spawn(livekit_video_thread(video, publication, sender));
    commands
        .entity(entity)
        .insert((LivekitTrackTask(handle), VideoFrameReceiver { receiver }));
}

#[cfg(not(target_arch = "wasm32"))]
#[expect(clippy::type_complexity, reason = "Queries are complex")]
fn receive_video_frame(
    mut commands: Commands,
    video_tracks: Populated<
        (Entity, &LivekitTrack, &mut VideoFrameReceiver, &PublishedBy),
        (With<Video>, With<Subscribed>),
    >,
    participants: Query<(&LivekitParticipant, Option<&StreamImage>)>,
    mut images: ResMut<Assets<Image>>,
) {
    for (entity, livekit_track, mut video_frame_receiver, published_by) in video_tracks.into_inner()
    {
        use tokio::sync::mpsc::error::TryRecvError;

        let Ok((participant, maybe_stream_image)) = participants.get(published_by.get()) else {
            error!("Invalid PublishedBy relationship.");
            commands.send_event(AppExit::from_code(1));
            return;
        };
        let Some(stream_image) = maybe_stream_image else {
            debug!(
                "Participant {} ({}) has subscribed video track but no StreamImage.",
                participant.sid(),
                participant.identity()
            );
            continue;
        };

        match video_frame_receiver.try_recv() {
            Ok(frame) => {
                let Some(image) = images.get_mut(stream_image.id()) else {
                    error!("StreamImage holds an invalid handle.");
                    commands.send_event(AppExit::from_code(1));
                    return;
                };

                if image.width() != frame.width() || image.height() != frame.height() {
                    debug!("Resizing StreamImage image.");
                    image.resize(Extent3d {
                        width: frame.width(),
                        height: frame.height(),
                        depth_or_array_layers: 1,
                    });
                }
                image.data = Some(frame.rgba_data());
            }
            Err(TryRecvError::Empty) => (),
            Err(TryRecvError::Disconnected) => {
                info!("Video stream {} is disconnected.", livekit_track.sid());
                commands.entity(entity).try_remove::<VideoFrameReceiver>();
            }
        }
    }
}

fn track_of_watched_streamer_published<C: Component>(
    trigger: Trigger<OnAdd, C>,
    mut commands: Commands,
    tracks: Query<&PublishedBy, With<C>>,
    participants: Query<Has<StreamBroadcast>, With<LivekitParticipant>>,
) {
    let entity = trigger.target();
    let Ok(published_by) = tracks.get(entity) else {
        error!("Malformed track.");
        commands.send_event(AppExit::from_code(1));
        return;
    };
    let Ok(has_stream_broadcast) = participants.get(published_by.get()) else {
        error!("Invalid PublishedBy relationship.");
        commands.send_event(AppExit::from_code(1));
        return;
    };

    if has_stream_broadcast {
        commands.trigger_targets(SubscribeToTrack, entity);
    }
}
