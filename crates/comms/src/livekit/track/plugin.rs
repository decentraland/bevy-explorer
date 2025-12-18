use bevy::{
    asset::RenderAssetUsages,
    ecs::relationship::Relationship,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use common::util::AsH160;
use livekit::track::{RemoteTrack, TrackKind, TrackSource};
use tokio::sync::{mpsc, oneshot};

use crate::{
    global_crdt::{GlobalCrdtState, PlayerMessage, PlayerUpdate},
    livekit::{
        kira_bridge::kira_thread,
        livekit_video_bridge::livekit_video_thread,
        participant::{HostedBy, LivekitParticipant},
        plugin::{PlayerUpdateTask, PlayerUpdateTasks},
        room::LivekitRoom,
        track::{
            Audio, Camera, LivekitFrame, LivekitTrack, LivekitTrackTask, Microphone,
            OpenAudioSender, OpenVideoSender, PublishedBy, SubscribeToAudioTrack,
            SubscribeToVideoTrack, Subscribed, Subscribing, TrackPublished, TrackSubscribed,
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
        app.add_observer(subscribe_to_video_track);
        app.add_observer(unsubscribe_to_track);

        app.add_systems(
            Update,
            (
                subscribed_audio_track_with_open_sender,
                subscribed_video_track_with_open_sender,
            ),
        );
    }
}

fn track_published(
    trigger: Trigger<TrackPublished>,
    mut commands: Commands,
    participants: Query<(Entity, &LivekitParticipant, &HostedBy)>,
    rooms: Query<&LivekitRuntime, With<LivekitRoom>>,
    player_state: Res<GlobalCrdtState>,
    mut player_update_tasks: ResMut<PlayerUpdateTasks>,
    mut images: ResMut<Assets<Image>>,
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
    let Ok(runtime) = rooms.get(room_entity) else {
        error!("Room {} does not have a runtime.", room_entity);
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
            let image = Image::new_fill(
                Extent3d {
                    width: 8,
                    height: 8,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                &[255, 0, 255, 255],
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::all(),
            );

            entity_cmd.insert((
                Video,
                LivekitFrame {
                    handle: images.add(image),
                },
            ));
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
        let task = runtime.spawn(async move {
            sender
                .send(PlayerUpdate {
                    transport_id: room_entity,
                    message: PlayerMessage::AudioStreamAvailable {
                        transport: room_entity,
                    },
                    address,
                })
                .await
        });
        player_update_tasks.push(PlayerUpdateTask {
            runtime: runtime.clone(),
            task,
        });
    }
}

fn track_unpublished(
    trigger: Trigger<TrackUnpublished>,
    mut commands: Commands,
    tracks: Query<(Entity, &LivekitTrack, &PublishedBy)>,
    participants: Query<(Entity, &LivekitParticipant, &HostedBy)>,
    rooms: Query<&LivekitRuntime, With<LivekitRoom>>,
    player_state: Res<GlobalCrdtState>,
    mut player_update_tasks: ResMut<PlayerUpdateTasks>,
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
    let Ok(runtime) = rooms.get(room_entity) else {
        error!("Room {} does not have a runtime.", room_entity);
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

    let maybe_address = participant.identity().as_str().as_h160();
    if maybe_address.is_some() && track.kind() == TrackKind::Audio {
        #[expect(
            clippy::unnecessary_unwrap,
            reason = "No let chains in current version."
        )]
        let address = maybe_address.unwrap();

        let sender = player_state.get_sender();
        let task = runtime.spawn(async move {
            sender
                .send(PlayerUpdate {
                    transport_id: room_entity,
                    message: PlayerMessage::AudioStreamUnavailable {
                        transport: room_entity,
                    },
                    address,
                })
                .await
        });

        player_update_tasks.push(PlayerUpdateTask {
            runtime: runtime.clone(),
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
) {
    let entity = trigger.target();
    let SubscribeToAudioTrack { runtime, sender } = trigger.event_mut();

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

    let track = track.clone();
    let (mut snatcher_sender, _) = oneshot::channel();
    std::mem::swap(&mut snatcher_sender, sender);

    debug!("Subscribing to audio track {}", track.sid());
    let task = runtime.spawn(async move {
        track.set_subscribed(true);
    });
    commands.entity(entity).insert((
        Subscribing { task },
        OpenAudioSender {
            runtime: runtime.clone(),
            sender: snatcher_sender,
        },
    ));
}

fn subscribe_to_video_track(
    mut trigger: Trigger<SubscribeToVideoTrack>,
    mut commands: Commands,
    tracks: Query<&LivekitTrack, With<Video>>,
) {
    let entity = trigger.target();
    let SubscribeToVideoTrack { runtime, sender } = trigger.event_mut();

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

    let track = track.clone();
    let (mut snatcher_sender, _) = mpsc::channel(1);
    std::mem::swap(&mut snatcher_sender, sender);

    debug!("Subscribing to video track {}", track.sid());
    let task = runtime.spawn(async move {
        track.set_subscribed(true);
    });
    commands.entity(entity).insert((
        Subscribing { task },
        OpenVideoSender {
            runtime: runtime.clone(),
            sender: snatcher_sender,
        },
    ));
}

fn unsubscribe_to_track(
    mut trigger: Trigger<UnsubscribeToTrack>,
    mut commands: Commands,
    tracks: Query<&LivekitTrack>,
) {
    let entity = trigger.target();
    let UnsubscribeToTrack { runtime } = trigger.event_mut();

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
    let task = runtime.spawn(async move {
        track.set_subscribed(false);
    });
    commands.entity(entity).insert(Unsubscribing { task });
}

#[expect(clippy::type_complexity, reason = "Queries are complex")]
fn subscribed_audio_track_with_open_sender(
    mut commands: Commands,
    mut tracks: Populated<
        (Entity, &LivekitTrack, &mut OpenAudioSender),
        (With<Audio>, With<Subscribed>),
    >,
) {
    for (entity, track, mut sender) in tracks.iter_mut() {
        let runtime = sender.runtime.clone();
        let publication = track.track.clone();

        let Some(RemoteTrack::Audio(audio)) = track.track() else {
            error!("A subscribed audio track did not have a audio RemoteTrack.");
            commands.send_event(AppExit::from_code(1));
            return;
        };

        let (mut snatcher_sender, _) = oneshot::channel();
        std::mem::swap(&mut snatcher_sender, &mut sender.sender);

        let handle = runtime.spawn(kira_thread(audio, publication, snatcher_sender));
        commands
            .entity(entity)
            .insert(LivekitTrackTask(handle))
            .remove::<OpenAudioSender>();
    }
}

#[expect(clippy::type_complexity, reason = "Queries are complex")]
fn subscribed_video_track_with_open_sender(
    mut commands: Commands,
    mut tracks: Populated<
        (Entity, &LivekitTrack, &mut OpenVideoSender),
        (With<Video>, With<Subscribed>),
    >,
) {
    for (entity, track, mut sender) in tracks.iter_mut() {
        let runtime = sender.runtime.clone();
        let publication = track.track.clone();

        let Some(RemoteTrack::Video(video)) = track.track() else {
            error!("A subscribed video track did not have a audio RemoteTrack.");
            commands.send_event(AppExit::from_code(1));
            return;
        };

        let (mut snatcher_sender, _) = mpsc::channel(1);
        std::mem::swap(&mut snatcher_sender, &mut sender.sender);

        let handle = runtime.spawn(livekit_video_thread(video, publication, snatcher_sender));
        commands
            .entity(entity)
            .insert(LivekitTrackTask(handle))
            .remove::<OpenVideoSender>();
    }
}
