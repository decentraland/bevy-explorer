//! The private chat (DM) room: one LiveKit room per environment that every DM-capable client joins
//! at login and stays in across realm changes. DMs are addressed by data-packet topic and
//! destination identity on it; its participant list is the DM presence list.
//!
//! The room is deliberately not a [`Transport`](crate::Transport): nothing broadcast to the
//! realm's transports can reach it, and its participants never touch the avatar store.

use alloy_core::primitives::Address;
use bevy::{
    ecs::relationship::Relationship,
    prelude::*,
    tasks::{IoTaskPool, Task},
};
use bevy_console::ConsoleCommand;
use common::{
    base_domain::Service,
    structs::{AppConfig, DmPrivacy, PrimaryUser},
    util::{AsH160, TaskCompat, TaskExt},
};
use console::DoAddConsoleCommand;
use dcl_component::proto_components::kernel::comms::rfc4;
use http::Uri;
use ipfs::IpfsAssetServer;
use prost::Message as _;
use serde::Deserialize;
use system_bridge::SystemApi;
use tokio::sync::mpsc::Sender;
use wallet::Wallet;

use crate::{
    chat_reaction::{
        canonical_message_id, emoji_to_atlas_index, encode_wire_index, received_emoji,
        ChatReactionEvent,
    },
    global_crdt::{ChannelControl, ChatEvent},
    livekit::{
        participant::{
            plugin::InboundRateLimiter, HostedBy, HostingParticipants, LivekitParticipant,
            Local as LocalParticipant, ParticipantIndex, ParticipantMetadataChanged,
            ParticipantPayload,
        },
        room::{Connected, Connecting, Disconnected},
        standalone_room_components, LivekitTransport,
    },
    mint_gatekeeper_adapter, DisableRealmComms, DisableSceneRoomGatekeeper, NetworkMessage,
    NetworkMessageRecipient,
};

pub struct PrivateChatPlugin;

impl Plugin for PrivateChatPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<PrivateChatReceived>();
        app.add_event::<PrivateChatPresenceChanged>();
        app.add_systems(
            Update,
            (
                connect_private_chat_room,
                send_private_chats,
                send_private_reactions,
            ),
        );
        app.add_observer(participant_added);
        app.add_observer(participant_removed);
        app.add_observer(participant_metadata_changed);
        app.add_observer(participant_payload);
        app.add_console_command::<DmRoomCommand, _>(dm_room);
        app.add_console_command::<DmCommand, _>(dm);
    }
}

/// Marks the private chat room entity and holds its outgoing channel.
#[derive(Component)]
pub struct PrivateChatRoom {
    pub sender: Sender<NetworkMessage>,
    // keeps the room's control channel open; the room has no voice, so nothing is ever sent
    _control: Sender<ChannelControl>,
}

/// On a private chat room participant: the DM privacy the server stamped into their participant
/// metadata, `None` until it arrives.
#[derive(Component, Debug)]
pub struct PrivateChatPrivacy(pub Option<DmPrivacy>);

/// A wallet joined or left the private chat room, or its privacy changed.
#[derive(Event, Debug)]
pub struct PrivateChatPresenceChanged {
    pub address: Address,
}

/// A DM addressed to the local user, after the topic and rate checks.
#[derive(Event, Debug)]
pub struct PrivateChatReceived {
    pub from: Address,
    pub message: String,
    /// The sender's rfc4 timestamp, which names the message for reactions.
    pub timestamp: f64,
}

#[derive(Deserialize)]
struct ParticipantMetadata {
    private_messages_privacy: String,
}

fn privacy_from_metadata(metadata: &str) -> Option<DmPrivacy> {
    let metadata = serde_json::from_str::<ParticipantMetadata>(metadata).ok()?;
    match metadata.private_messages_privacy.as_str() {
        "all" => Some(DmPrivacy::All),
        "only_friends" => Some(DmPrivacy::OnlyFriends),
        _ => None,
    }
}

/// Gatekeeper retry backoff: 5s doubling to 60s.
fn gatekeeper_retry_delay(attempt: u32) -> f64 {
    (5.0 * 2f64.powi(attempt.min(4) as i32)).min(60.0)
}

#[allow(clippy::too_many_arguments)]
fn connect_private_chat_room(
    mut commands: Commands,
    wallet: Res<Wallet>,
    config: Res<AppConfig>,
    ipfs: IpfsAssetServer,
    time: Res<Time>,
    rooms: Query<Entity, With<PrivateChatRoom>>,
    mut gatekeeper_task: Local<Option<Task<Result<String, anyhow::Error>>>>,
    mut retry: Local<Option<(f64, u32)>>,
    disable_realm_comms: Option<Res<DisableRealmComms>>,
    disable_gatekeeper: Res<DisableSceneRoomGatekeeper>,
) {
    // headless servers never join (they would be a ghost participant), and orchestrated servers
    // never sign gatekeeper handshakes themselves
    if disable_realm_comms.is_some_and(|d| d.0) || disable_gatekeeper.0 {
        return;
    }

    let now = time.elapsed_secs_f64();
    let mint = if wallet.is_changed() {
        for room in rooms.iter() {
            commands.entity(room).despawn();
        }
        *gatekeeper_task = None;
        *retry = None;

        if wallet.address().is_none() {
            info!("private chat: no identity, not connecting");
            return;
        }
        true
    } else {
        gatekeeper_task.is_none() && retry.is_some_and(|(due, _)| now >= due)
    };

    if mint {
        let uri = Uri::try_from(common::base_domain::url(
            Service::CommsGatekeeper,
            "/private-messages/token",
        ))
        .unwrap();
        let meta = serde_json::json!({
            "signer": "dcl:explorer",
            "deviceIdentifier": config.user_id,
        })
        .to_string();
        let client = ipfs.ipfs().client();
        let wallet = wallet.clone();
        *gatekeeper_task = Some(IoTaskPool::get().spawn_compat(async move {
            mint_gatekeeper_adapter("GET", client, uri, &wallet, meta).await
        }));
    }

    if let Some(mut task) = gatekeeper_task.take() {
        match task.complete() {
            None => *gatekeeper_task = Some(task),
            Some(Err(e)) => {
                let attempt = retry.map_or(0, |(_, attempt)| attempt);
                let delay = gatekeeper_retry_delay(attempt);
                warn!(
                    "private chat: failed to get room from gatekeeper: {e}; retrying in {delay}s"
                );
                *retry = Some((now + delay, attempt + 1));
            }
            Some(Ok(adapter)) => {
                *retry = None;
                let Some(("livekit", address)) = adapter.split_once(':') else {
                    warn!("private chat: unsupported adapter `{adapter}`");
                    return;
                };
                let (components, sender, control) = standalone_room_components(address.to_owned());
                commands.spawn((
                    components,
                    PrivateChatRoom {
                        sender,
                        _control: control,
                    },
                ));
                info!("private chat: connecting");
            }
        }
    }
}

/// Stamps a private chat room participant with the privacy from their metadata.
fn participant_added(
    trigger: Trigger<OnAdd, LivekitParticipant>,
    mut commands: Commands,
    participants: Query<(&LivekitParticipant, &HostedBy)>,
    rooms: Query<(), With<PrivateChatRoom>>,
    mut presence: EventWriter<PrivateChatPresenceChanged>,
) {
    let Ok((participant, hosted_by)) = participants.get(trigger.target()) else {
        return;
    };
    if !rooms.contains(hosted_by.get()) {
        return;
    }
    let privacy = privacy_from_metadata(&participant.metadata());
    debug!(target: "comms::private_chat", "{} joined, privacy {privacy:?}", participant.identity());
    commands
        .entity(trigger.target())
        .try_insert(PrivateChatPrivacy(privacy));
    if let Some(address) = participant.identity().as_str().as_h160() {
        presence.write(PrivateChatPresenceChanged { address });
    }
}

fn participant_removed(
    trigger: Trigger<OnRemove, PrivateChatPrivacy>,
    participants: Query<&LivekitParticipant>,
    mut presence: EventWriter<PrivateChatPresenceChanged>,
) {
    let Ok(participant) = participants.get(trigger.target()) else {
        return;
    };
    if let Some(address) = participant.identity().as_str().as_h160() {
        presence.write(PrivateChatPresenceChanged { address });
    }
}

fn participant_metadata_changed(
    trigger: Trigger<ParticipantMetadataChanged>,
    rooms: Query<&ParticipantIndex, With<PrivateChatRoom>>,
    mut participants: Query<&mut PrivateChatPrivacy>,
    mut presence: EventWriter<PrivateChatPresenceChanged>,
) {
    let ParticipantMetadataChanged { participant, room } = trigger.event();
    let Ok(index) = rooms.get(*room) else {
        return;
    };
    let Some(entity) = index
        .get(&ParticipantIndex::key(participant.identity().as_str()))
        .copied()
    else {
        return;
    };
    let Ok(mut privacy) = participants.get_mut(entity) else {
        return;
    };
    privacy.0 = privacy_from_metadata(&participant.metadata());
    debug!(target: "comms::private_chat", "{} privacy {:?}", participant.identity(), privacy.0);
    if let Some(address) = participant.identity().as_str().as_h160() {
        presence.write(PrivateChatPresenceChanged { address });
    }
}

/// A DM or a reaction to one on the private chat room. The sender is the LiveKit-authenticated
/// identity; the topic must name us, so a packet addressed elsewhere (or to nobody) is dropped.
fn participant_payload(
    trigger: Trigger<ParticipantPayload>,
    rooms: Query<(), With<PrivateChatRoom>>,
    wallet: Res<Wallet>,
    time: Res<Time>,
    mut rate_limiter: ResMut<InboundRateLimiter>,
    mut received: EventWriter<PrivateChatReceived>,
    mut reactions: EventWriter<ChatReactionEvent>,
) {
    let ParticipantPayload {
        room,
        participant,
        payload,
        topic,
    } = trigger.event();
    if !rooms.contains(*room) {
        return;
    }
    let identity = participant.identity().to_string();
    let Some(from) = identity.as_h160() else {
        return;
    };
    if !rate_limiter.allow(*room, &identity, time.elapsed_secs_f64()) {
        return;
    }
    let Some(me) = wallet.address() else {
        return;
    };
    if topic
        .as_deref()
        .is_none_or(|topic| topic.as_h160() != Some(me))
    {
        warn!(target: "comms::private_chat", "dropping packet from {from:#x} with topic {topic:?}");
        return;
    }
    let message = match rfc4::Packet::decode(payload.as_slice()) {
        Ok(rfc4::Packet {
            message: Some(rfc4::packet::Message::Chat(chat)),
            ..
        }) => chat,
        Ok(rfc4::Packet {
            message: Some(rfc4::packet::Message::ChatReaction(reaction)),
            ..
        }) => {
            debug!(target: "comms::private_chat", "dm reaction from {from:#x}: {reaction:?}");
            if let (Some(message_id), Some((emoji, remove))) = (
                canonical_message_id(&reaction.message_id),
                received_emoji(&reaction.emoji, reaction.emoji_index, reaction.remove),
            ) {
                reactions.write(ChatReactionEvent {
                    channel: format!("{from:#x}"),
                    message_id,
                    emoji,
                    from,
                    remove,
                });
            }
            return;
        }
        Ok(other) => {
            debug!(target: "comms::private_chat", "ignoring non-chat packet from {from:#x}: {other:?}");
            return;
        }
        Err(e) => {
            warn!(target: "comms::private_chat", "undecodable packet from {from:#x}: {e}");
            return;
        }
    };
    debug!(target: "comms::private_chat", "dm from {from:#x}: {}", message.message);
    received.write(PrivateChatReceived {
        from,
        message: message.message,
        timestamp: message.timestamp,
    });
}

/// The local player's chat events whose channel is a wallet address go out as DMs on the private
/// chat room, addressed to that wallet by topic and destination identity.
fn send_private_chats(
    mut chats: EventReader<ChatEvent>,
    player: Query<Entity, With<PrimaryUser>>,
    room: Query<&PrivateChatRoom>,
) {
    let Ok(player) = player.single() else {
        return;
    };
    for ev in chats.read().filter(|ev| ev.sender == player) {
        let Some(to) = ev.channel.as_h160() else {
            continue;
        };
        let Ok(room) = room.single() else {
            warn!(target: "comms::private_chat", "no private chat room, dropping dm to {to:#x}");
            continue;
        };
        let packet = rfc4::Packet {
            message: Some(rfc4::packet::Message::Chat(rfc4::Chat {
                message: ev.message.clone(),
                timestamp: ev.timestamp,
            })),
            protocol_version: 100,
        };
        let message = NetworkMessage {
            topic: Some(format!("{to:#x}")),
            ..NetworkMessage::targetted_reliable(&packet, NetworkMessageRecipient::Peer(to))
        };
        match room.sender.try_send(message) {
            Ok(()) => debug!(target: "comms::private_chat", "dm to {to:#x}: {}", ev.message),
            Err(e) => warn!(target: "comms::private_chat", "failed to queue dm to {to:#x}: {e}"),
        }
    }
}

/// The local user's reactions to DMs go to the partner the same way the DMs do.
fn send_private_reactions(
    mut reactions: EventReader<ChatReactionEvent>,
    wallet: Res<Wallet>,
    room: Query<&PrivateChatRoom>,
) {
    let Some(me) = wallet.address() else {
        reactions.clear();
        return;
    };
    for ev in reactions.read().filter(|ev| ev.from == me) {
        let Some(to) = ev.channel.as_h160() else {
            continue;
        };
        let Ok(room) = room.single() else {
            warn!(target: "comms::private_chat", "no private chat room, dropping reaction to {to:#x}");
            continue;
        };
        let packet = rfc4::Packet {
            message: Some(rfc4::packet::Message::ChatReaction(rfc4::ChatReaction {
                emoji_index: Some(encode_wire_index(
                    emoji_to_atlas_index(&ev.emoji),
                    ev.remove,
                )),
                message_id: ev.message_id.clone(),
                address: format!("{me:#x}"),
                emoji: ev.emoji.clone(),
                remove: ev.remove,
            })),
            protocol_version: 100,
        };
        let message = NetworkMessage {
            topic: Some(format!("{to:#x}")),
            ..NetworkMessage::targetted_reliable(&packet, NetworkMessageRecipient::Peer(to))
        };
        if let Err(e) = room.sender.try_send(message) {
            warn!(target: "comms::private_chat", "failed to queue reaction to {to:#x}: {e}");
        }
    }
}

/// `/dm <address> <message>` sends a DM, the same way the HUD does.
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/dm")]
struct DmCommand {
    address: String,
    #[arg(trailing_var_arg = true, required = true)]
    message: Vec<String>,
}

fn dm(mut input: ConsoleCommand<DmCommand>, mut events: EventWriter<SystemApi>) {
    let Some(Ok(command)) = input.take() else {
        return;
    };
    if command.address.as_h160().is_none() {
        input.reply_failed(format!("`{}` is not a wallet address", command.address));
        return;
    }
    let message = command.message.join(" ");
    events.write(SystemApi::SendChat(
        message.clone(),
        command.address.clone(),
    ));
    input.reply_ok(format!("dm to {}: {message}", command.address));
}

/// `/dm_room` prints the private chat room's connection state and who is in it.
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/dm_room")]
struct DmRoomCommand;

#[allow(clippy::type_complexity)]
fn dm_room(
    mut input: ConsoleCommand<DmRoomCommand>,
    rooms: Query<
        (
            &LivekitTransport,
            Has<Connecting>,
            Has<Connected>,
            Has<Disconnected>,
            Option<&HostingParticipants>,
        ),
        With<PrivateChatRoom>,
    >,
    participants: Query<
        (&LivekitParticipant, Option<&PrivateChatPrivacy>),
        Without<LocalParticipant>,
    >,
    wallet: Res<Wallet>,
) {
    const LISTED: usize = 20;

    if input.take().is_none() {
        return;
    }
    let Ok((transport, connecting, connected, disconnected, hosting)) = rooms.single() else {
        input.reply_ok("private chat room: not spawned");
        return;
    };
    let state = match (connecting, connected, disconnected) {
        (_, true, _) => "connected",
        (true, _, _) => "connecting",
        (_, _, true) => "disconnected",
        _ => "pending",
    };
    let host = transport.address.split('?').next().unwrap_or_default();
    let identity = wallet
        .address()
        .map(|a| format!("{a:#x}"))
        .unwrap_or_else(|| "none".to_owned());
    let remote: Vec<_> = hosting
        .map(|hosting| participants.iter_many(hosting.collection()).collect())
        .unwrap_or_default();
    let mut lines = vec![
        format!("private chat room: {state} ({host})"),
        format!("identity: {identity}"),
        format!("remote participants: {}", remote.len()),
    ];
    lines.extend(remote.iter().take(LISTED).map(|(participant, privacy)| {
        format!(
            "  {} {:?}",
            participant.identity(),
            privacy.map(|p| p.0).unwrap_or_default()
        )
    }));
    if remote.len() > LISTED {
        lines.push(format!("  ... and {} more", remote.len() - LISTED));
    }
    input.reply_ok(lines.join("\n"));
}
