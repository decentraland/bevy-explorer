use bevy::{
    ecs::{component::HookContext, relationship::Relationship, world::DeferredWorld},
    prelude::*,
};
use common::{structs::AudioDecoderError, util::AsH160};
use kira::sound::streaming::StreamingSoundData;
use livekit::{
    prelude::{Participant, RemoteTrackPublication},
    track::{RemoteTrack, TrackKind, TrackSource},
};
use tokio::sync::oneshot;
#[cfg(not(target_arch = "wasm32"))]
use tokio::task::JoinHandle;

use crate::{
    global_crdt::{GlobalCrdtState, PlayerMessage, PlayerUpdate},
    livekit::{
        kira_bridge::kira_thread,
        participant::{HostedBy, LivekitParticipant},
        plugin::{PlayerUpdateTask, PlayerUpdateTasks},
        room::LivekitRoom,
        LivekitRuntime,
    },
    make_hooks,
};

#[derive(Clone, Component, Deref, DerefMut)]
pub struct LivekitTrack {
    track: RemoteTrackPublication,
}

#[derive(Component)]
#[component(on_replace=Self::on_replace)]
struct LivekitTrackTask(JoinHandle<()>);

impl LivekitTrackTask {
    fn on_replace(mut deferred_world: DeferredWorld, hook_context: HookContext) {
        let entity = hook_context.entity;

        let mut entity_mut = deferred_world.entity_mut(entity);
        let task = entity_mut
            .get_mut::<LivekitTrackTask>()
            .expect("LivekitTrackTask must be valid inside its own hook.");
        task.0.abort();
    }
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
#[component(on_add=Self::on_add, on_replace=Self::on_replace)]
pub struct Subscribing {
    #[cfg(not(target_arch = "wasm32"))]
    task: JoinHandle<()>,
}
make_hooks!(Subscribing, (Subscribed, Unsubscribed, Unsubscribing));

impl Subscribing {
    fn on_replace(mut deferred_world: DeferredWorld, hook_context: HookContext) {
        let entity = hook_context.entity;

        let mut entity_mut = deferred_world.entity_mut(entity);
        let task = entity_mut
            .get_mut::<Subscribing>()
            .expect("Subscribing must be valid inside its own hook.");
        task.task.abort();
    }
}

#[derive(Component)]
#[component(on_add=Self::on_add, on_replace=Self::on_replace)]
pub struct Unsubscribing {
    #[cfg(not(target_arch = "wasm32"))]
    task: JoinHandle<()>,
}
make_hooks!(Unsubscribing, (Subscribed, Unsubscribed, Subscribing));

impl Unsubscribing {
    fn on_replace(mut deferred_world: DeferredWorld, hook_context: HookContext) {
        let entity = hook_context.entity;

        let mut entity_mut = deferred_world.entity_mut(entity);
        let task = entity_mut
            .get_mut::<Unsubscribing>()
            .expect("Unsubscribing must be valid inside its own hook.");
        task.task.abort();
    }
}

#[derive(Component)]
struct OpenSender {
    runtime: LivekitRuntime,
    sender: oneshot::Sender<StreamingSoundData<AudioDecoderError>>,
}

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

#[derive(Event)]
pub struct SubscribeToTrack {
    pub runtime: LivekitRuntime,
    pub sender: oneshot::Sender<StreamingSoundData<AudioDecoderError>>,
}

#[derive(Event)]
pub struct UnsubscribeToTrack {
    pub runtime: LivekitRuntime,
}

pub(super) struct LivekitTrackPlugin;

impl Plugin for LivekitTrackPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(track_published);
        app.add_observer(track_unpublished);
        app.add_observer(track_subscribed);
        app.add_observer(track_unsubscribed);
        app.add_observer(subscribe_to_track);
        app.add_observer(unsubscribe_to_track);

        app.add_systems(Update, subscribed_audio_track_with_open_sender);
    }
}

fn track_published(
    trigger: Trigger<TrackPublished>,
    mut commands: Commands,
    participants: Query<(Entity, &LivekitParticipant, &HostedBy)>,
    rooms: Query<&LivekitRuntime, With<LivekitRoom>>,
    player_state: Res<GlobalCrdtState>,
    mut player_update_tasks: ResMut<PlayerUpdateTasks>,
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

fn subscribe_to_track(
    mut trigger: Trigger<SubscribeToTrack>,
    mut commands: Commands,
    tracks: Query<&LivekitTrack>,
) {
    let entity = trigger.target();
    let SubscribeToTrack { runtime, sender } = trigger.event_mut();

    if entity == Entity::PLACEHOLDER {
        error!("SubscribeToTrack is an entity event. Call it with 'Commands::trigger_targets'.");
        return;
    }
    let Ok(track) = tracks.get(entity) else {
        error!("Can't subscribe to {} because it is not a track.", entity);
        return;
    };

    let track = track.clone();
    let (mut snatcher_sender, _) = oneshot::channel();
    std::mem::swap(&mut snatcher_sender, sender);

    debug!("Subscribing to track {}", track.sid());
    let task = runtime.spawn(async move {
        track.set_subscribed(true);
    });
    commands.entity(entity).insert((
        Subscribing { task },
        OpenSender {
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
        (Entity, &LivekitTrack, &mut OpenSender),
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
            .remove::<OpenSender>();
    }
}
