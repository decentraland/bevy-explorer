#[cfg(target_arch = "wasm32")]
use std::sync::atomic::Ordering;

use bevy::{
    ecs::{error::debug, relationship::Relationship, system::entity_command},
    prelude::*,
    render::render_resource::Extent3d,
};
use common::{debug_panic, util::AsH160};
use livestream_manager::{
    ActiveAudioTransmitter, ActiveTransmitter, ActiveVideoCast, AudioTransmitterKind,
    AudioTransmitterVolume, TransmissionUpdated, TransmitterKind,
};
#[cfg(not(target_arch = "wasm32"))]
use {
    bevy::ecs::world::OnDespawn,
    kira::sound::streaming::StreamingSoundData,
    livekit::{
        track::{RemoteTrack, TrackKind, TrackSource},
        webrtc::video_frame::VideoBuffer,
    },
    tokio::sync::{mpsc, oneshot},
};
#[cfg(target_arch = "wasm32")]
use {
    bevy::render::renderer::WgpuWrapper,
    common::{structs::AudioSettings, util::ReportErr},
    media::{FrameCopyRequest, FrameCopyRequestQueue, HtmlMedia},
    web_sys::VideoFrame,
};

#[cfg(not(target_arch = "wasm32"))]
use crate::livekit::{
    kira_bridge::kira_thread,
    livekit_bridge::{livekit_video_thread, AudioTrackKiraBridge, I420BufferExt},
    track::{AudioStreamingHandle, LivekitTrackTask, OpenAudioSender, VideoFrameReceiver},
    LivekitAudioManager,
};
#[cfg(target_arch = "wasm32")]
use crate::livekit::{
    track::HtmlMediaEntity,
    web::{RemoteTrack, TrackKind, TrackSource},
};
use crate::{
    global_crdt::{PlayerMessage, PlayerUpdate},
    livekit::{
        participant::{ActiveSpeaker, HostedBy, LivekitParticipant},
        plugin::{PlayerUpdateTask, PlayerUpdateTasks},
        track::{
            Audio, Camera, LivekitTrack, Microphone, PublishedBy, ScreenshareAudio,
            ScreenshareVideo, SubscribeToAudioTrack, SubscribeToTrack, Subscribed, Subscribing,
            TrackPublished, TrackSubscribed, TrackUnpublished, TrackUnsubscribed,
            UnsubscribeToTrack, Unsubscribed, Unsubscribing, Video,
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
        app.add_systems(Update, subscribed_audio_track_with_open_sender);
        #[cfg(not(target_arch = "wasm32"))]
        app.add_observer(audio_track_is_now_subscribed);
        #[cfg(not(target_arch = "wasm32"))]
        app.add_observer(audio_track_unpublished);
        app.add_observer(video_track_is_now_subscribed);
        #[cfg(target_arch = "wasm32")]
        app.add_observer(video_track_is_now_unsubscribed);

        app.add_observer(active_transmitter_added);
        app.add_observer(active_transmitter_removed);

        #[cfg(not(target_arch = "wasm32"))]
        app.add_systems(Update, copy_frame);
        #[cfg(target_arch = "wasm32")]
        app.add_systems(Update, queue_frame_copy);

        app.add_observer(on_active_audio_transmitter_add);
        app.add_observer(on_active_audio_transmitter_remove);

        app.add_systems(Update, update_track_volume);
    }
}

fn track_published(
    trigger: Trigger<TrackPublished>,
    mut commands: Commands,
    participants: Query<(Entity, &LivekitParticipant, &HostedBy, Has<ActiveSpeaker>)>,
    transport_senders: crate::global_crdt::TransportSenders,
    mut player_update_tasks: ResMut<PlayerUpdateTasks>,
    livekit_runtime: Res<LivekitRuntime>,
) {
    let TrackPublished { participant, track } = trigger.event();

    let Some((participant_entity, _, hosted_by, has_active_speaker)) = participants
        .iter()
        .find(|(_, livekit_participant, _, _)| livekit_participant.sid() == participant.sid())
    else {
        debug_panic!("No participant entity with sid {}.", participant.sid());
    };

    let room_entity = hosted_by.get();

    let identity = participant.identity();
    let identity_str = identity.as_str();

    debug!(
        "{} ({}) published {:?} ({:?}) track {}.",
        participant.sid(),
        identity,
        track.kind(),
        track.source(),
        track.sid(),
    );
    let mut entity_cmd = commands.spawn((
        LivekitTrack {
            track: track.clone(),
        },
        PublishedBy(participant_entity),
        Unsubscribed,
    ));
    match track.kind() {
        TrackKind::Audio => {
            entity_cmd.try_insert(Audio);
        }
        TrackKind::Video => {
            entity_cmd.try_insert(Video);
        }
    }
    match track.source() {
        TrackSource::Microphone => {
            entity_cmd.try_insert(Microphone);
            if identity_str.starts_with("presentation-bot:") || identity_str.starts_with("stream:")
            {
                entity_cmd.insert(AudioTransmitterKind::Cast);
            } else if identity_str.ends_with("-streamer") {
                entity_cmd.insert(AudioTransmitterKind::Stream);
            }
        }
        TrackSource::Camera => {
            entity_cmd.try_insert(Camera);
            if identity_str.starts_with("stream:") {
                entity_cmd.try_insert(TransmitterKind::VideoCast);
                if has_active_speaker {
                    entity_cmd.try_insert(ActiveVideoCast);
                }
            } else if identity_str.ends_with("-streamer") {
                entity_cmd.try_insert(TransmitterKind::Stream);
            }
        }
        TrackSource::ScreenshareAudio => {
            entity_cmd.try_insert(ScreenshareAudio);
            if identity_str.starts_with("presentation-bot:") || identity_str.starts_with("stream:")
            {
                entity_cmd.insert(AudioTransmitterKind::Cast);
            } else if identity_str.ends_with("-streamer") {
                entity_cmd.insert(AudioTransmitterKind::Stream);
            }
        }
        TrackSource::Screenshare => {
            entity_cmd.try_insert(ScreenshareVideo);
            if identity_str.starts_with("presentation-bot:") {
                entity_cmd.try_insert(TransmitterKind::Presentation);
            } else {
                entity_cmd.try_insert(TransmitterKind::Screenshare);
            }
        }
        source => warn!("Track {} had {:?} source.", track.sid(), source),
    }

    let maybe_address = identity_str.as_h160();
    if track.kind() == TrackKind::Audio && maybe_address.is_some() {
        #[expect(
            clippy::unnecessary_unwrap,
            reason = "No let chains in current version."
        )]
        let address = maybe_address.unwrap();

        let Some(sender) = transport_senders.get(room_entity) else {
            return;
        };
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
            runtime: (*livekit_runtime).clone(),
            task,
        });
    }
}

fn track_unpublished(
    trigger: Trigger<TrackUnpublished>,
    mut commands: Commands,
    tracks: Query<(Entity, &LivekitTrack, &PublishedBy)>,
    participants: Query<(Entity, &LivekitParticipant, &HostedBy)>,
    transport_senders: crate::global_crdt::TransportSenders,
    mut player_update_tasks: ResMut<PlayerUpdateTasks>,
    livekit_runtime: Res<LivekitRuntime>,
) {
    let TrackUnpublished { participant, track } = trigger.event();

    let Some((participant_entity, _, hosted_by)) = participants
        .iter()
        .find(|(_, livekit_participant, _)| livekit_participant.sid() == participant.sid())
    else {
        debug_panic!("No participant entity with sid {}.", participant.sid());
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
        debug_panic!("No track entity with sid {}.", track.sid());
    };

    if published_by.get() != participant_entity {
        debug_panic!(
            "Unpublished track {} was not published by {}.",
            track.sid(),
            participant.sid()
        );
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

        let Some(sender) = transport_senders.get(room_entity) else {
            return;
        };
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
            runtime: (*livekit_runtime).clone(),
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
        debug_panic!("No subscribing track with sid {}.", track.sid());
    };

    debug!("Subscribed to track {}.", track.sid());
    commands.entity(entity).try_insert(Subscribed);
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
        debug_panic!("No unsubscribing track with sid {}.", track.sid());
    };

    debug!("Unsubscribed to track {}.", track.sid());
    commands.entity(entity).try_insert(Unsubscribed);
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
        error!(
            "Can't subscribe to audio track {} because it is not a track.",
            entity
        );
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
    commands.entity(entity).try_insert((
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
    tracks: Query<(&LivekitTrack, AnyOf<(&Audio, &Video)>)>,
    livekit_runtime: Res<LivekitRuntime>,
) {
    let entity = trigger.target();

    if entity == Entity::PLACEHOLDER {
        error!("SubscribeToTrack is an entity event. Call it with 'Commands::trigger_targets'.");
        return;
    }
    let Ok((track, audio_or_video)) = tracks.get(entity) else {
        error!("Can't subscribe to {} because it is not a track.", entity);
        return;
    };

    let track = track.clone();

    let kind = match audio_or_video {
        (Some(_), None) => "audio",
        (None, Some(_)) => "video",
        _ => panic!("Track must have either Audio or Video."),
    };

    debug!("Subscribing to {kind} track {}", track.sid());
    let task = livekit_runtime.spawn(async move {
        track.set_subscribed(true);
    });
    commands.entity(entity).try_insert(Subscribing { task });
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
        error!("Can't unsubscribe to {} because it is not a track. This may happen when changing scenes.", entity);
        return;
    };

    let track = track.clone();

    debug!("Unsubscribing to track {}", track.sid());
    let task = livekit_runtime.spawn(async move {
        track.set_subscribed(false);
    });
    commands.entity(entity).try_insert(Unsubscribing { task });
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
            debug_panic!("A subscribed audio track did not have a audio RemoteTrack.");
        };

        let (mut snatcher_sender, _) = oneshot::channel();
        std::mem::swap(&mut snatcher_sender, &mut sender.sender);

        let handle = runtime.spawn(kira_thread(audio, publication, snatcher_sender));
        commands
            .entity(entity)
            .try_insert(LivekitTrackTask(handle))
            .remove::<OpenAudioSender>();
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[expect(clippy::type_complexity, reason = "Queries are complex")]
fn audio_track_is_now_subscribed(
    trigger: Trigger<OnAdd, Subscribed>,
    mut commands: Commands,
    tracks: Query<
        (
            &LivekitTrack,
            Has<Audio>,
            Has<AudioStreamingHandle>,
            Has<OpenAudioSender>,
        ),
        With<Subscribed>,
    >,
    mut livekit_audio_manager: ResMut<LivekitAudioManager>,
) {
    let entity = trigger.target();
    let Ok((track, is_audio, has_audio_streaming_sound, has_open_audio_sender)) =
        tracks.get(entity)
    else {
        debug_panic!("Subscribed track did not have LivekitTrack.");
    };
    if !is_audio {
        trace!("Subscribed track was not an audio track.");
        return;
    }
    if has_audio_streaming_sound {
        // Creation of [`AudioStreamingSound`] only needs to be done once
        // for a track
        return;
    }
    if has_open_audio_sender {
        // Spatial audio track
        return;
    }

    let Some(RemoteTrack::Audio(audio)) = track.track() else {
        debug_panic!("A subscribed audio track did not have a audio RemoteTrack.");
    };

    let decoder = AudioTrackKiraBridge::new(audio, 48_000);

    let Ok(handle) = livekit_audio_manager.play(StreamingSoundData::from_decoder(decoder)) else {
        error!("Failed to play track audio in LivekitAudioManager.");
        return;
    };

    commands
        .entity(entity)
        .try_insert(AudioStreamingHandle { handle });
}

#[cfg(not(target_arch = "wasm32"))]
fn audio_track_unpublished(
    trigger: Trigger<OnDespawn, Audio>,
    mut tracks: Query<(&LivekitTrack, Option<&mut AudioStreamingHandle>), With<Audio>>,
) {
    let entity = trigger.target();
    let Ok((livekit_track, maybe_audio_streaming_handle)) = tracks.get_mut(entity) else {
        debug_panic!("Audio track did not have LivekitTrack.");
    };

    let Some(mut audio_streaming_sound) = maybe_audio_streaming_handle else {
        trace!(
            "Audio track {} did not have AudioStreamingHandle.",
            livekit_track.sid()
        );
        return;
    };

    debug!("Stopping audio track {}.", livekit_track.sid());
    audio_streaming_sound.stop(Default::default());
}

#[cfg(not(target_arch = "wasm32"))]
fn video_track_is_now_subscribed(
    trigger: Trigger<OnAdd, Subscribed>,
    mut commands: Commands,
    tracks: Query<(&LivekitTrack, Has<Video>), With<Subscribed>>,
    livekit_runtime: Res<LivekitRuntime>,
) {
    let entity = trigger.target();
    let Ok((track, is_video)) = tracks.get(entity) else {
        debug_panic!("Subscribed track did not have LivekitTrack.");
    };
    if !is_video {
        trace!("Subscribed track was not a video track.");
        return;
    }

    let runtime = livekit_runtime.clone();
    let publication = track.track.clone();

    let Some(RemoteTrack::Video(video)) = track.track() else {
        debug_panic!("A subscribed video track did not have a video RemoteTrack.");
    };

    let (sender, receiver) = mpsc::channel(60);
    let handle = runtime.spawn(livekit_video_thread(video, publication, sender));
    commands
        .entity(entity)
        .try_insert((LivekitTrackTask(handle), VideoFrameReceiver { receiver }));
}

#[cfg(target_arch = "wasm32")]
#[expect(clippy::type_complexity)]
fn video_track_is_now_subscribed(
    trigger: Trigger<OnAdd, Subscribed>,
    mut commands: Commands,
    tracks: Query<(&LivekitTrack, Has<Video>, Option<&ActiveTransmitter>), With<Subscribed>>,
) {
    let entity = trigger.target();
    let Ok((track, is_video, maybe_active_transmitter)) = tracks.get(entity) else {
        debug_panic!("Subscribed track did not have LivekitTrack.");
    };
    if !is_video {
        trace!("Subscribed track was not a video track.");
        return;
    }
    let Some(active_transmitter) = maybe_active_transmitter else {
        debug!("Video subscribbed to without being active transmitter.");
        return;
    };

    let Some(RemoteTrack::Video(video)) = track.track() else {
        debug_panic!("A subscribed video track did not have a video RemoteTrack.");
    };

    let Some(video_element) = video.html_video_element() else {
        debug!("Could not build HtmlMedia from livekit track.");
        return;
    };
    let html_media =
        HtmlMedia::video_from_element(video_element, String::new(), (*active_transmitter).clone());
    commands.entity(entity).try_insert(HtmlMediaEntity {
        element: html_media,
    });
}

#[cfg(target_arch = "wasm32")]
fn video_track_is_now_unsubscribed(
    trigger: Trigger<OnReplace, Subscribed>,
    mut commands: Commands,
) {
    let entity = trigger.target();
    commands.entity(entity).try_remove::<HtmlMediaEntity>();
}

#[cfg(target_arch = "wasm32")]
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn queue_frame_copy(
    mut video_tracks: Query<&mut HtmlMediaEntity, (With<Video>, With<Subscribed>)>,
    mut images: ResMut<Assets<Image>>,
    send_queue: Res<FrameCopyRequestQueue>,
    mut transmission_updated: EventWriter<TransmissionUpdated>,
) {
    for mut html_media_entity in video_tracks.iter_mut() {
        #[allow(clippy::collapsible_else_if)]
        if let Some(video) = html_media_entity.video().as_ref() {
            let new_time = html_media_entity
                .new_frame_time()
                .swap(0, Ordering::Relaxed);
            if new_time != 0 {
                // new frame is ready
                let new_time = f32::from_bits(new_time);
                trace!("got new frame -> {new_time}");

                let Ok(frame) = VideoFrame::new_with_html_video_element(video) else {
                    warn!("failed to extract frame");
                    continue;
                };

                let image_id = html_media_entity.image().as_ref().unwrap().id();
                let visible_rect = frame.visible_rect().unwrap();
                let video_size = (visible_rect.width() as u32, visible_rect.height() as u32);

                // check size
                if html_media_entity.size().is_none_or(|sz| sz != video_size) {
                    let Some(image) = images.get_mut(image_id) else {
                        continue;
                    };
                    debug!("Resizing active transmitter image.");
                    image.resize(Extent3d {
                        width: video_size.0,
                        height: video_size.1,
                        depth_or_array_layers: 1,
                    });
                    html_media_entity.set_size(Some(video_size));

                    trace!("queue resized frame {:?}", video_size);
                    transmission_updated.write(TransmissionUpdated);
                }

                // queue copy
                trace!("queue frame {:?}", video_size);
                send_queue
                    .send(FrameCopyRequest {
                        video_frame: WgpuWrapper::new(frame),
                        target: image_id,
                    })
                    .report();

                html_media_entity.set_current_time(new_time);
            } else {
                trace!("no frame (new_time == 0)");
            }
        } else {
            debug!("no video");
            // we don't report audio timestamps, otherwise would need to grab it here
        }
    }
}

fn active_transmitter_added(trigger: Trigger<OnAdd, ActiveTransmitter>, mut commands: Commands) {
    let entity = trigger.target();
    commands.entity(entity).trigger(SubscribeToTrack);
}

fn active_transmitter_removed(
    trigger: Trigger<OnRemove, ActiveTransmitter>,
    mut commands: Commands,
) {
    let entity = trigger.target();
    commands.entity(entity).queue_handled(
        entity_command::trigger(UnsubscribeToTrack),
        bevy::ecs::error::debug,
    );
}

#[cfg(not(target_arch = "wasm32"))]
fn copy_frame(
    mut commands: Commands,
    active_transmitter: Single<(Entity, &ActiveTransmitter, &mut VideoFrameReceiver)>,
    mut images: ResMut<Assets<Image>>,
    mut transmission_updated: EventWriter<TransmissionUpdated>,
) {
    let (entity, active_transmitter, mut video_frame_receiver): (
        Entity,
        &ActiveTransmitter,
        Mut<VideoFrameReceiver>,
    ) = active_transmitter.into_inner();

    let Some(image) = images.get_mut(active_transmitter.id()) else {
        debug_panic!("ActiveTransmitter image handle is invalid");
    };

    let mut frame = None;
    loop {
        match video_frame_receiver.try_recv() {
            Ok(new_frame) => frame = Some(new_frame),
            Err(mpsc::error::TryRecvError::Empty) => break,
            Err(mpsc::error::TryRecvError::Disconnected) => {
                error!("VideoFrameReceiver has disconnected.");
                commands.entity(entity).try_remove::<VideoFrameReceiver>();
                break;
            }
        }
    }
    if let Some(frame) = frame {
        if image.width() != frame.width() || image.height() != frame.height() {
            debug!("Resizing active transmitter image.");
            image.resize(Extent3d {
                width: frame.width(),
                height: frame.height(),
                depth_or_array_layers: 1,
            });
            transmission_updated.write(TransmissionUpdated);
        }
        if let Some(data) = &mut image.data {
            // TODO verify this transfer priority
            image.transfer_priority = bevy::asset::RenderAssetTransferPriority::Priority(-2);
            frame.rgba_data_into_slice(data.as_mut_slice());
        } else {
            image.data = Some(frame.rgba_data());
        }
    }
}

fn on_active_audio_transmitter_add(
    trigger: Trigger<OnAdd, ActiveAudioTransmitter>,
    mut commands: Commands,
    tracks: Query<(), (With<LivekitTrack>, With<Audio>)>,
) {
    let entity = trigger.target();
    if !tracks.contains(entity) {
        debug!("ActiveAudioTransmitter added to something that is not an LivekitTrack.");
        return;
    }

    commands.entity(entity).trigger(SubscribeToTrack);
}

fn on_active_audio_transmitter_remove(
    trigger: Trigger<OnRemove, ActiveAudioTransmitter>,
    mut commands: Commands,
    tracks: Query<(), (With<LivekitTrack>, With<Audio>)>,
) {
    let entity = trigger.target();
    if !tracks.contains(entity) {
        return;
    }

    commands
        .entity(entity)
        .queue_handled(entity_command::trigger(UnsubscribeToTrack), debug);
}

#[cfg(not(target_arch = "wasm32"))]
fn update_track_volume(tracks: Populated<(&mut AudioStreamingHandle, &AudioTransmitterVolume)>) {
    use kira::tween::Tween;
    for (mut audio_streaming_handle, audio_transmitter_volume) in tracks.into_inner() {
        audio_streaming_handle
            .handle
            .set_volume(**audio_transmitter_volume as f64, Tween::default());
    }
}

#[cfg(target_arch = "wasm32")]
#[expect(clippy::type_complexity)]
fn update_track_volume(
    tracks: Populated<(&LivekitTrack, &AudioTransmitterVolume), (With<Audio>, With<Subscribed>)>,
    audio_settings: Res<AudioSettings>,
) {
    for (livekit_track, audio_transmitter_volume) in tracks.into_inner() {
        let Some(RemoteTrack::Audio(audio)) = livekit_track.track() else {
            debug_panic!("A subscribed audio track did not have an audio RemoteTrack.");
        };
        audio.set_volume(**audio_transmitter_volume * audio_settings.scene());
    }
}
