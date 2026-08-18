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
use prost::Message;
use system_bridge::VoiceMessage;
#[cfg(not(target_arch = "wasm32"))]
use {
    bevy::{
        asset::RenderAssetUsages,
        color::palettes,
        render::render_resource::{TextureDimension, TextureFormat, TextureUsages},
    },
    livekit::prelude::Participant,
};

#[cfg(not(target_arch = "wasm32"))]
use crate::livekit::participant::{StreamImage, StreamViewer};
#[cfg(target_arch = "wasm32")]
use crate::livekit::web::Participant;
use crate::{
    global_crdt::{
        NetworkUpdate, NonPlayerUpdate, PlayerMessage, PlayerUpdate, VoiceMessageStreams,
    },
    livekit::{
        participant::{
            ActiveSpeaker, ActiveSpeakersChanged, ChangeVolume, HostedBy, HostingParticipants,
            LivekitParticipant, Local, ParticipantConnected, ParticipantConnectionQuality,
            ParticipantDisconnected, ParticipantMetadataChanged, ParticipantPayload,
            StreamBroadcast, Streamer,
        },
        plugin::{PlayerUpdateTask, PlayerUpdateTasks},
        room::LivekitRoom,
        track::{Audio, LivekitTrack, Publishing, SubscribeToTrack, TrackVolume, Video},
        LivekitRuntime,
    },
    SceneRoom,
};

const INBOUND_RATE_WINDOW_SECS: f64 = 1.0;
const MAX_MESSAGES_PER_WINDOW: usize = 300;

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

        #[cfg(not(target_arch = "wasm32"))]
        app.add_systems(
            Update,
            (
                stream_viewer_without_stream_image,
                non_stream_viewer_with_stream_image,
            ),
        );
        app.add_observer(someone_wants_to_watch_stream);
        app.add_observer(noone_is_watching_stream);
        app.add_observer(change_volume_of_tracks);
    }
}

fn participant_connected(
    trigger: Trigger<ParticipantConnected>,
    mut commands: Commands,
    rooms: Query<&LivekitRoom>,
    transport_senders: crate::global_crdt::TransportSenders,
    mut player_update_tasks: ResMut<PlayerUpdateTasks>,
    livekit_runtime: Res<LivekitRuntime>,
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

    if is_local {
        commands.spawn((
            participant.clone(),
            <HostedBy as Relationship>::from(*room_entity),
            Local,
        ));
    } else if participant.identity().as_str().ends_with("-streamer") {
        commands.spawn((
            participant.clone(),
            <HostedBy as Relationship>::from(*room_entity),
            Streamer,
        ));
    } else {
        commands.spawn((
            participant.clone(),
            <HostedBy as Relationship>::from(*room_entity),
        ));
    }

    // Register presence explicitly. Membership is otherwise implied by a `PlayerUpdate` arriving,
    // and the only one this path used to emit was the metadata message below — which is skipped
    // when the token carries no metadata, so a peer whose avatar state rides Pulse could sit in the
    // room without ever being recorded as being in it. `PlayerLeft` on disconnect is the mirror.
    if !is_local {
        if let Some(address) = participant.identity().as_str().as_h160() {
            let transport_id = *room_entity;
            if let Some(sender) = transport_senders.get(transport_id) {
                let task = livekit_runtime.spawn(async move {
                    sender
                        .send(
                            PlayerUpdate {
                                transport_id,
                                message: PlayerMessage::Joined,
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
    transport_senders: crate::global_crdt::TransportSenders,
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
        // the transport (or its context) may already be torn down; skip only the
        // notification — the participant entity below must still be despawned
        if let Some(sender) = transport_senders.get(transport_id) {
            let task = livekit_runtime.spawn(async move {
                sender
                    .send(NetworkUpdate::PlayerLeft {
                        transport_id,
                        address,
                    })
                    .await
            });
            player_update_tasks.push(PlayerUpdateTask {
                runtime: livekit_runtime.clone(),
                task,
            });
        }
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
    transport_senders: crate::global_crdt::TransportSenders,
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

    // Where avatar state already arrives over Pulse, drop the legacy LiveKit copies so the two
    // inbound pipes don't both drive the foreign avatar (double-applied position / double-played
    // emote). Gated on the *receiving context* rather than unconditionally: an authoritative server
    // never joins Pulse and these packets — which clients send it with
    // `NetworkMessageRecipient::AuthServer` — are its only source of player presence. SceneEmote is
    // unaffected either way.
    if transport_senders.is_pulse_fed(room)
        && matches!(
            message,
            rfc4::packet::Message::Movement(_)
                | rfc4::packet::Message::MovementCompressed(_)
                | rfc4::packet::Message::PlayerEmote(_)
        )
    {
        return;
    }

    let Some(sender) = transport_senders.get(room) else {
        return;
    };

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
        runtime: livekit_runtime.clone(),
        task,
    });
}

fn participant_metadata_changed(
    trigger: Trigger<ParticipantMetadataChanged>,
    transport_senders: crate::global_crdt::TransportSenders,
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
            let Some(sender) = transport_senders.get(room) else {
                return;
            };
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
                runtime: livekit_runtime.clone(),
                task,
            });
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn stream_viewer_without_stream_image(
    mut commands: Commands,
    stream_viewers: Populated<(Entity, &StreamViewer), Without<StreamImage>>,
    stream_broadcasts: Query<&StreamImage, With<StreamBroadcast>>,
) {
    for (entity, stream_viewer) in stream_viewers.into_inner() {
        let Ok(stream_image) = stream_broadcasts.get(stream_viewer.get()) else {
            debug_panic!("Invalid StreamBroadcast relationship.");
        };

        commands.entity(entity).try_insert(stream_image.clone());
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[expect(clippy::type_complexity, reason = "Queries are complex")]
fn non_stream_viewer_with_stream_image(
    mut commands: Commands,
    stream_viewers: Populated<
        Entity,
        (
            Without<StreamViewer>,
            Without<StreamBroadcast>,
            With<StreamImage>,
        ),
    >,
) {
    for entity in stream_viewers.into_inner() {
        commands.entity(entity).remove::<StreamImage>();
    }
}

fn someone_wants_to_watch_stream(
    trigger: Trigger<OnAdd, StreamBroadcast>,
    mut commands: Commands,
    participants: Query<(&LivekitParticipant, Option<&Publishing>), With<Streamer>>,
    audio_tracks: Query<(), With<Audio>>,
    video_tracks: Query<(), With<Video>>,
    #[cfg(not(target_arch = "wasm32"))] mut images: ResMut<Assets<Image>>,
) {
    let entity = trigger.target();
    let Ok((participant, maybe_publishing)) = participants.get(entity) else {
        debug_panic!("StreamBroadcast on a non-Streamer participant.");
    };

    debug!(
        "Streamer {} ({}) is now being watched.",
        participant.sid(),
        participant.identity()
    );
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut image = Image::new_fill(
            bevy::render::render_resource::Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &palettes::basic::FUCHSIA.to_u8_array(),
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::all(),
        );
        image.texture_descriptor.usage = TextureUsages::COPY_DST | TextureUsages::TEXTURE_BINDING;

        commands
            .entity(entity)
            .try_insert(StreamImage(images.add(image)));
    }

    if let Some(publishing) = maybe_publishing {
        if let Some(audio_track) = publishing
            .iter()
            .find(|published_track| audio_tracks.contains(*published_track))
        {
            commands.trigger_targets(SubscribeToTrack, audio_track);
        } else {
            debug!(
                "Participant {} ({}) is being watched but do not have any published audio track.",
                participant.sid(),
                participant.identity()
            );
        }
        if let Some(video_track) = publishing
            .iter()
            .find(|published_track| video_tracks.contains(*published_track))
        {
            commands.trigger_targets(SubscribeToTrack, video_track);
        } else {
            debug!(
                "Participant {} ({}) is being watched but do not have any published video track.",
                participant.sid(),
                participant.identity()
            );
        }
    }
}

fn noone_is_watching_stream(
    trigger: Trigger<OnRemove, StreamBroadcast>,
    #[cfg(not(target_arch = "wasm32"))] mut commands: Commands,
    participants: Query<(&LivekitParticipant, Option<&Publishing>), With<Streamer>>,
    tracks: Query<&LivekitTrack>,
    livekit_runtime: Res<LivekitRuntime>,
) {
    let entity = trigger.target();
    let Ok((participant, maybe_publishing)) = participants.get(entity) else {
        debug_panic!("StreamBroadcast on a non-Streamer participant.");
    };
    debug!(
        "Streamer {} ({}) no longer being watched.",
        participant.sid(),
        participant.identity()
    );
    #[cfg(not(target_arch = "wasm32"))]
    commands.entity(entity).try_remove::<StreamImage>();

    if let Some(publishing) = maybe_publishing {
        for livekit_track in tracks.iter_many(publishing.collection()) {
            let track = livekit_track.clone();
            livekit_runtime.spawn(async move {
                track.set_subscribed(false);
            });
        }
    }
}

fn change_volume_of_tracks(
    trigger: Trigger<ChangeVolume>,
    mut commands: Commands,
    participants: Query<&Publishing>,
    tracks: Query<(), With<Audio>>,
) {
    let entity = trigger.target();
    let event = trigger.event();

    let Ok(publishing) = participants.get(entity) else {
        error!("{} is not publishing any tracks.", entity);
        return;
    };

    for track in publishing.collection() {
        if !tracks.contains(*track) {
            continue;
        }

        commands.entity(*track).try_insert(TrackVolume(event.0));
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

fn is_now_speaking(
    trigger: Trigger<OnInsert, ActiveSpeaker>,
    participants: Query<(&LivekitParticipant, Option<&HostedBy>), With<ActiveSpeaker>>,
    scene_rooms: Query<&SceneRoom>,
    senders: Res<VoiceMessageStreams>,
) {
    let entity = trigger.target();

    let Ok((participant, maybe_hosted_by)) = participants.get(entity) else {
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

    let channel = match scene_rooms.get(room.get()).ok() {
        Some(room) => room.0.clone(),
        None => "Nearby".to_string(),
    };
    for sender in senders.iter() {
        if let Some(sender_address) = participant.identity().as_str().as_h160() {
            sender
                .send(VoiceMessage {
                    sender_address: format!("{:#x}", sender_address),
                    channel: channel.clone(),
                    active: true,
                })
                .report();
        } else {
            error!(
                "Non-h160 participant {} ({}) tried to send voice data.",
                participant.sid(),
                participant.identity()
            );
        }
    }
}

fn is_no_longer_speaking(
    trigger: Trigger<OnReplace, ActiveSpeaker>,
    participants: Query<(&LivekitParticipant, Option<&HostedBy>), With<ActiveSpeaker>>,
    scene_rooms: Query<&SceneRoom>,
    senders: Res<VoiceMessageStreams>,
) {
    let entity = trigger.target();

    let Ok((participant, maybe_hosted_by)) = participants.get(entity) else {
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

    let channel = match scene_rooms.get(room.get()).ok() {
        Some(room) => room.0.clone(),
        None => "Nearby".to_string(),
    };
    for sender in senders.iter() {
        if let Some(sender_address) = participant.identity().as_str().as_h160() {
            sender
                .send(VoiceMessage {
                    sender_address: format!("{:#x}", sender_address),
                    channel: channel.clone(),
                    active: false,
                })
                .report();
        } else {
            error!(
                "Non-h160 participant {} ({}) tried to send voice data.",
                participant.sid(),
                participant.identity()
            );
        }
    }
}
