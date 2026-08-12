use std::collections::{HashMap, VecDeque};

use bevy::{
    ecs::{entity::EntityHashSet, relationship::Relationship},
    platform::collections::HashSet,
    prelude::*,
};
use common::{
    debug_panic,
    util::{AsH160, ReportErr},
};
use dcl_component::proto_components::kernel::comms::rfc4;
#[cfg(not(target_arch = "wasm32"))]
use livekit::prelude::Participant;
use livestream_manager::ActiveVideoCast;
use prost::Message;
use system_bridge::VoiceMessage;

#[cfg(target_arch = "wasm32")]
use crate::livekit::web::Participant;
use crate::{
    global_crdt::{
        GlobalCrdtState, NetworkUpdate, NonPlayerUpdate, PlayerMessage, PlayerUpdate,
        VoiceMessageStreams,
    },
    livekit::{
        participant::{
            ActiveSpeaker, ActiveSpeakersChanged, HostedBy, HostingParticipants,
            LivekitParticipant, Local, ParticipantConnected, ParticipantConnectionQuality,
            ParticipantDisconnected, ParticipantMetadataChanged, ParticipantPayload,
        },
        plugin::{PlayerUpdateTask, PlayerUpdateTasks},
        room::LivekitRoom,
        track::{Camera as CameraTrack, Publishing, Video},
        LivekitRuntime,
    },
    SceneRoom,
};

const INBOUND_RATE_WINDOW_SECS: f64 = 1.0;
const MAX_MESSAGES_PER_WINDOW: usize = 300;
const GRACE_PERIOD: f32 = 3.;

// Per-peer sliding-window rate limit; entries are evicted when the participant entity is removed,
// which bounds the map by the number of connected participants. Keyed per room as well as
// identity so the same identity string in two rooms (island + scene) can't share a window.
#[derive(Resource, Default)]
struct InboundRateLimiter {
    windows: HashMap<(Entity, String), VecDeque<f64>>,
}

impl InboundRateLimiter {
    fn allow(&mut self, room: Entity, identity: &str, now: f64) -> bool {
        let cutoff = now - INBOUND_RATE_WINDOW_SECS;

        let times = self.windows.entry((room, identity.to_owned())).or_default();
        while times.front().is_some_and(|&t| t < cutoff) {
            times.pop_front();
        }
        if times.len() >= MAX_MESSAGES_PER_WINDOW {
            return false;
        }
        times.push_back(now);
        true
    }
}

pub struct LivekitParticipantPlugin;

impl Plugin for LivekitParticipantPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InboundRateLimiter>();
        app.add_observer(participant_connected);
        app.add_observer(participant_disconnected);
        app.add_observer(participant_entity_removed);
        app.add_observer(participant_connection_quality_changed);
        app.add_observer(participant_payload);
        app.add_observer(participant_metadata_changed);
        app.add_observer(active_speakers_changed);
        app.add_observer(is_now_speaking);
        app.add_observer(is_no_longer_speaking);

        app.add_systems(Update, verify_active_speaker_grace_period);
    }
}

#[derive(Component, Deref, DerefMut)]
struct ActiveSpeakerGracePeriod(Timer);

fn participant_connected(
    trigger: Trigger<ParticipantConnected>,
    mut commands: Commands,
    rooms: Query<&LivekitRoom>,
) {
    let ParticipantConnected {
        participant,
        room: room_entity,
    } = trigger.event();
    let Ok(room) = rooms.get(*room_entity) else {
        debug_panic!("Room {room_entity} given to ParticipantConnected was invalid.");
    };
    debug!(
        "Participant '{}' ({}) connected to room {}.",
        participant.sid(),
        participant.identity(),
        room.name()
    );

    let is_local = matches!(participant.participant, Participant::Local(_));

    let mut cmd = commands.spawn((
        participant.clone(),
        <HostedBy as Relationship>::from(*room_entity),
    ));
    if is_local {
        cmd.insert(Local);
    }

    commands.trigger(ParticipantMetadataChanged {
        room: *room_entity,
        participant: participant.clone(),
    });
}

fn participant_disconnected(
    trigger: Trigger<ParticipantDisconnected>,
    mut commands: Commands,
    participants: Query<(Entity, &LivekitParticipant)>,
    rooms: Query<(&LivekitRoom, Option<&HostingParticipants>)>,
    global_crdt_state: Res<GlobalCrdtState>,
    mut player_update_tasks: ResMut<PlayerUpdateTasks>,
    livekit_runtime: Res<LivekitRuntime>,
) {
    let ParticipantDisconnected {
        participant,
        room: room_entity,
    } = trigger.event();
    let Ok((room, maybe_hosting_participants)) = rooms.get(*room_entity) else {
        debug_panic!("Room {room_entity} given to ParticipantDisconnected was invalid.");
    };
    debug!(
        "Participant '{}' ({}) disconnected from room {}.",
        participant.sid(),
        participant.identity(),
        room.name()
    );

    if let Some(address) = participant.identity().as_str().as_h160() {
        let transport_id = *room_entity;
        let sender = global_crdt_state.get_sender();
        let task = livekit_runtime.spawn(async move {
            sender
                .send(NetworkUpdate::PlayerLeft {
                    transport_id,
                    address,
                })
                .await
        });
        player_update_tasks.push(PlayerUpdateTask {
            runtime: (*livekit_runtime).clone(),
            task,
        });
    }

    let Some(hosting_participants) = maybe_hosting_participants else {
        debug_panic!("Room {} is not hosting participants.", room.name());
    };

    let Some(entity) = participants
        .iter_many(hosting_participants.collection())
        .find_map(|(entity, ecs_participant)| {
            if ecs_participant.sid() == participant.sid() {
                Some(entity)
            } else {
                None
            }
        })
    else {
        error!(
            "Disconnecting participant '{}' ({}) not found in participants.",
            participant.sid(),
            participant.identity()
        );
        return;
    };

    commands.entity(entity).despawn();
}

// Covers both explicit disconnects and relationship-cascade despawns on room teardown, so
// rate-limiter entries can't outlive their participant.
fn participant_entity_removed(
    trigger: Trigger<OnRemove, LivekitParticipant>,
    participants: Query<(&LivekitParticipant, Option<&HostedBy>)>,
    mut rate_limiter: ResMut<InboundRateLimiter>,
) {
    let Ok((participant, maybe_hosted)) = participants.get(trigger.target()) else {
        return;
    };
    let identity = participant.identity();
    match maybe_hosted {
        Some(hosted) => {
            rate_limiter
                .windows
                .remove(&(hosted.get(), identity.as_str().to_owned()));
        }
        // no room to scope by; over-evicting just forgets ≤1s of history
        None => rate_limiter
            .windows
            .retain(|(_, id), _| id != identity.as_str()),
    }
}

fn participant_connection_quality_changed(
    trigger: Trigger<ParticipantConnectionQuality>,
    mut commands: Commands,
    participants: Query<(Entity, &LivekitParticipant)>,
    rooms: Query<(&LivekitRoom, &HostingParticipants)>,
) {
    let ParticipantConnectionQuality {
        participant,
        room,
        connection_quality,
    } = trigger.event();
    let Ok((livekit_room, hosting_participants)) = rooms.get(*room) else {
        debug_panic!("Room given to ParticipantConnectionQuality was invalid.");
    };

    debug!(
        "Participant '{}' ({}) connection quality with {} changed to {:?}.",
        participant.sid(),
        participant.identity(),
        livekit_room.name(),
        connection_quality
    );

    let Some(entity) = participants
        .iter_many(hosting_participants.collection())
        .find_map(|(entity, ecs_participant)| {
            if ecs_participant.sid() == participant.sid() {
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

    commands.entity(entity).try_insert(*connection_quality);
}

fn participant_payload(
    trigger: Trigger<ParticipantPayload>,
    global_crdt_state: Res<GlobalCrdtState>,
    mut player_update_tasks: ResMut<PlayerUpdateTasks>,
    livekit_runtime: Res<LivekitRuntime>,
    mut rate_limiter: ResMut<InboundRateLimiter>,
    time: Res<Time>,
) {
    let ParticipantPayload {
        room: room_entity,
        participant,
        payload,
    } = trigger.event();

    if !rate_limiter.allow(
        *room_entity,
        participant.identity().as_str(),
        time.elapsed_secs_f64(),
    ) {
        trace!(
            "rate-limited payload from participant {} ({}).",
            participant.sid(),
            participant.identity()
        );
        return;
    }

    let packet = match rfc4::Packet::decode(payload.as_slice()) {
        Ok(packet) => packet,
        Err(_) => {
            warn!(
                "Could not decode payload from participant {} ({}).",
                participant.sid(),
                participant.identity()
            );
            return;
        }
    };
    let Some(message) = packet.message else {
        warn!(
            "Payload from {} ({}) had empty body.",
            participant.sid(),
            participant.identity()
        );
        return;
    };
    let room = *room_entity;
    let sender = global_crdt_state.get_sender();

    let task = if let Some(address) = participant.identity().as_str().as_h160() {
        trace!(
            "[{}] received [{}] packet {message:?} from {address}",
            room_entity,
            packet.protocol_version
        );
        livekit_runtime.spawn(async move {
            sender
                .send(
                    PlayerUpdate {
                        transport_id: room,
                        message: PlayerMessage::PlayerData(message),
                        address,
                    }
                    .into(),
                )
                .await
        })
    } else {
        let address = participant.identity().to_string();
        livekit_runtime.spawn(async move {
            sender
                .send(
                    NonPlayerUpdate {
                        transport_id: room,
                        address,
                        message,
                    }
                    .into(),
                )
                .await
        })
    };

    player_update_tasks.push(PlayerUpdateTask {
        runtime: (*livekit_runtime).clone(),
        task,
    });
}

fn participant_metadata_changed(
    trigger: Trigger<ParticipantMetadataChanged>,
    global_crdt_state: Res<GlobalCrdtState>,
    mut player_update_tasks: ResMut<PlayerUpdateTasks>,
    livekit_runtime: Res<LivekitRuntime>,
) {
    let ParticipantMetadataChanged { room, participant } = trigger.event();

    let meta = participant.metadata();
    if !meta.is_empty() {
        debug!(
            "Metadata of {} ({}) changed.",
            participant.sid(),
            participant.identity()
        );
        if let Some(address) = participant.identity().as_str().as_h160() {
            let room = *room;
            let sender = global_crdt_state.get_sender();
            let task = livekit_runtime.spawn(async move {
                sender
                    .send(
                        PlayerUpdate {
                            transport_id: room,
                            message: PlayerMessage::MetaData(meta),
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
}

fn active_speakers_changed(
    trigger: Trigger<ActiveSpeakersChanged>,
    mut commands: Commands,
    participants: Query<(Entity, &LivekitParticipant)>,
    old_speakers: Query<Entity, With<ActiveSpeaker>>,
) {
    let ActiveSpeakersChanged { speakers } = trigger.event();

    let active_speakers_sid = speakers
        .iter()
        .map(|participant| participant.sid())
        .collect::<HashSet<_>>();

    let active_speakers = participants
        .iter()
        .filter(|(_, livekit_participant)| active_speakers_sid.contains(&livekit_participant.sid()))
        .map(|(entity, _)| entity)
        .collect::<EntityHashSet>();

    let old_speakers = old_speakers.iter().collect::<EntityHashSet>();

    let new_speakers = active_speakers.difference(&old_speakers);
    let no_longer_speakers = old_speakers.difference(&active_speakers);

    for new_speaker in new_speakers {
        commands.entity(*new_speaker).try_insert(ActiveSpeaker);
    }
    for no_longer_speaker in no_longer_speakers {
        commands
            .entity(*no_longer_speaker)
            .try_remove::<ActiveSpeaker>();
    }
}

#[expect(clippy::type_complexity)]
fn is_now_speaking(
    trigger: Trigger<OnInsert, ActiveSpeaker>,
    mut commands: Commands,
    participants: Query<
        (&LivekitParticipant, Option<&HostedBy>, Option<&Publishing>),
        With<ActiveSpeaker>,
    >,
    tracks: Query<Has<ActiveVideoCast>, (With<Video>, With<CameraTrack>)>,
    scene_rooms: Query<&SceneRoom>,
    senders: Res<VoiceMessageStreams>,
) {
    let entity = trigger.target();

    let Ok((participant, maybe_hosted_by, maybe_publishing)) = participants.get(entity) else {
        unreachable!("Infallible Query");
    };
    debug!(
        "{} ({}) is now speaking.",
        participant.sid(),
        participant.identity()
    );

    let Some(room) = maybe_hosted_by else {
        debug_panic!(
            "{} ({}) is not hosted by a room.",
            participant.sid(),
            participant.identity()
        );
    };

    if let Some(sender_address) = participant.identity().as_str().as_h160() {
        let channel = match scene_rooms.get(room.get()).ok() {
            Some(room) => room.0.clone(),
            None => "Nearby".to_string(),
        };
        for sender in senders.iter() {
            sender
                .send(VoiceMessage {
                    sender_address: format!("{:#x}", sender_address),
                    channel: channel.clone(),
                    active: true,
                })
                .report();
        }
    } else if let Some(publishing) = maybe_publishing {
        for published in publishing.collection() {
            if let Ok(has_active_video_cast) = tracks.get(*published) {
                if has_active_video_cast {
                    commands
                        .entity(*published)
                        .try_remove::<ActiveSpeakerGracePeriod>();
                } else {
                    commands.entity(*published).insert(ActiveVideoCast);
                }
            }
        }
    }
}

#[expect(clippy::type_complexity)]
fn is_no_longer_speaking(
    trigger: Trigger<OnReplace, ActiveSpeaker>,
    mut commands: Commands,
    participants: Query<
        (&LivekitParticipant, Option<&HostedBy>, Option<&Publishing>),
        With<ActiveSpeaker>,
    >,
    tracks: Query<(), (With<Video>, With<CameraTrack>, With<ActiveVideoCast>)>,
    scene_rooms: Query<&SceneRoom>,
    senders: Res<VoiceMessageStreams>,
) {
    let entity = trigger.target();

    let Ok((participant, maybe_hosted_by, maybe_publishing)) = participants.get(entity) else {
        unreachable!("Infallible Query");
    };
    debug!(
        "{} ({}) is no longer speaking.",
        participant.sid(),
        participant.identity()
    );

    let Some(room) = maybe_hosted_by else {
        debug_panic!(
            "{} ({}) is not hosted by a room.",
            participant.sid(),
            participant.identity()
        );
    };

    if let Some(sender_address) = participant.identity().as_str().as_h160() {
        let channel = match scene_rooms.get(room.get()).ok() {
            Some(room) => room.0.clone(),
            None => "Nearby".to_string(),
        };
        for sender in senders.iter() {
            sender
                .send(VoiceMessage {
                    sender_address: format!("{:#x}", sender_address),
                    channel: channel.clone(),
                    active: false,
                })
                .report();
        }
    } else if let Some(publishing) = maybe_publishing {
        for published in publishing.collection() {
            if tracks.contains(*published) {
                commands
                    .entity(*published)
                    .try_insert(ActiveSpeakerGracePeriod(Timer::from_seconds(
                        GRACE_PERIOD,
                        TimerMode::Once,
                    )));
            }
        }
    }
}

fn verify_active_speaker_grace_period(
    mut commands: Commands,
    active_speakers: Populated<(Entity, &mut ActiveSpeakerGracePeriod)>,
    time: Res<Time<Real>>,
) {
    let delta = time.delta();
    for (entity, mut active_speaker_grace_period) in active_speakers.into_inner() {
        active_speaker_grace_period.tick(delta);
        if active_speaker_grace_period.finished() {
            debug!("Grace period for {} has passed.", entity);
            commands
                .entity(entity)
                .try_remove::<(ActiveSpeakerGracePeriod, ActiveVideoCast)>();
        }
    }
}
