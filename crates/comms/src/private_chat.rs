//! The private chat (DM) room: one LiveKit room per environment that every DM-capable client joins
//! at login and stays in across realm changes. DMs are addressed by data-packet topic and
//! destination identity on it; its participant list is the DM presence list.
//!
//! The room is deliberately not a [`Transport`](crate::Transport): nothing broadcast to the
//! realm's transports can reach it, and its participants never touch the avatar store. The
//! generic LiveKit participant observers skip it, so a busy room costs one map entry per
//! participant rather than an entity each.

use bevy::{
    platform::collections::HashMap,
    prelude::*,
    tasks::{IoTaskPool, Task},
};
use bevy_console::ConsoleCommand;
use common::{
    base_domain::Service,
    structs::{AppConfig, DmPrivacy},
    util::{AsH160, TaskCompat, TaskExt},
};
use console::DoAddConsoleCommand;
use http::Uri;
use ipfs::IpfsAssetServer;
use serde::Deserialize;
use tokio::sync::mpsc::Sender;
use wallet::Wallet;

use crate::{
    global_crdt::ChannelControl,
    livekit::{
        participant::{ParticipantConnected, ParticipantDisconnected, ParticipantMetadataChanged},
        room::{Connected, Connecting, Disconnected},
        standalone_room_components, LivekitTransport,
    },
    mint_gatekeeper_adapter, DisableRealmComms, DisableSceneRoomGatekeeper, NetworkMessage,
};

pub struct PrivateChatPlugin;

impl Plugin for PrivateChatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PrivateChatPresence>();
        app.add_systems(Update, connect_private_chat_room);
        app.add_observer(participant_connected);
        app.add_observer(participant_disconnected);
        app.add_observer(participant_metadata_changed);
        app.add_console_command::<DmRoomCommand, _>(dm_room);
    }
}

/// Marks the private chat room entity and holds its outgoing channel.
#[derive(Component)]
pub struct PrivateChatRoom {
    pub sender: Sender<NetworkMessage>,
    // keeps the room's control channel open; the room has no voice, so nothing is ever sent
    _control: Sender<ChannelControl>,
}

/// Remote participants of the private chat room, keyed by wallet, with the DM privacy the
/// server stamped into their participant metadata (`None` until metadata arrives).
#[derive(Resource, Default)]
pub struct PrivateChatPresence(pub HashMap<alloy_core::primitives::Address, Option<DmPrivacy>>);

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

#[allow(clippy::too_many_arguments)]
fn connect_private_chat_room(
    mut commands: Commands,
    wallet: Res<Wallet>,
    config: Res<AppConfig>,
    ipfs: IpfsAssetServer,
    rooms: Query<Entity, With<PrivateChatRoom>>,
    mut presence: ResMut<PrivateChatPresence>,
    mut gatekeeper_task: Local<Option<Task<Result<String, anyhow::Error>>>>,
    disable_realm_comms: Option<Res<DisableRealmComms>>,
    disable_gatekeeper: Res<DisableSceneRoomGatekeeper>,
) {
    // headless servers never join (they would be a ghost participant), and orchestrated servers
    // never sign gatekeeper handshakes themselves
    if disable_realm_comms.is_some_and(|d| d.0) || disable_gatekeeper.0 {
        return;
    }

    if wallet.is_changed() {
        for room in rooms.iter() {
            commands.entity(room).despawn();
        }
        presence.0.clear();
        *gatekeeper_task = None;

        if wallet.address().is_none() {
            info!("private chat: no identity, not connecting");
            return;
        }

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
            Some(Err(e)) => warn!("private chat: failed to get room from gatekeeper: {e}"),
            Some(Ok(adapter)) => {
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

fn participant_connected(
    trigger: Trigger<ParticipantConnected>,
    rooms: Query<(), With<PrivateChatRoom>>,
    wallet: Res<Wallet>,
    mut presence: ResMut<PrivateChatPresence>,
) {
    let ParticipantConnected { participant, room } = trigger.event();
    if !rooms.contains(*room) {
        return;
    }
    let Some(address) = participant.identity().as_str().as_h160() else {
        return;
    };
    if Some(address) == wallet.address() {
        return;
    }
    let privacy = privacy_from_metadata(&participant.metadata());
    debug!(target: "comms::private_chat", "{address:#x} joined, privacy {privacy:?}");
    presence.0.insert(address, privacy);
}

fn participant_disconnected(
    trigger: Trigger<ParticipantDisconnected>,
    rooms: Query<(), With<PrivateChatRoom>>,
    mut presence: ResMut<PrivateChatPresence>,
) {
    let ParticipantDisconnected { participant, room } = trigger.event();
    if !rooms.contains(*room) {
        return;
    }
    if let Some(address) = participant.identity().as_str().as_h160() {
        debug!(target: "comms::private_chat", "{address:#x} left");
        presence.0.remove(&address);
    }
}

fn participant_metadata_changed(
    trigger: Trigger<ParticipantMetadataChanged>,
    rooms: Query<(), With<PrivateChatRoom>>,
    mut presence: ResMut<PrivateChatPresence>,
) {
    let ParticipantMetadataChanged { participant, room } = trigger.event();
    if !rooms.contains(*room) {
        return;
    }
    let Some(address) = participant.identity().as_str().as_h160() else {
        return;
    };
    if let Some(privacy) = presence.0.get_mut(&address) {
        *privacy = privacy_from_metadata(&participant.metadata());
        debug!(target: "comms::private_chat", "{address:#x} privacy {privacy:?}");
    }
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
        ),
        With<PrivateChatRoom>,
    >,
    wallet: Res<Wallet>,
    presence: Res<PrivateChatPresence>,
) {
    const LISTED: usize = 20;

    if input.take().is_none() {
        return;
    }
    let Ok((transport, connecting, connected, disconnected)) = rooms.single() else {
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
    let mut lines = vec![
        format!("private chat room: {state} ({host})"),
        format!("identity: {identity}"),
        format!("remote participants: {}", presence.0.len()),
    ];
    lines.extend(
        presence
            .0
            .iter()
            .take(LISTED)
            .map(|(address, privacy)| format!("  {address:#x} {privacy:?}")),
    );
    if presence.0.len() > LISTED {
        lines.push(format!("  ... and {} more", presence.0.len() - LISTED));
    }
    input.reply_ok(lines.join("\n"));
}
