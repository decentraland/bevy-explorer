//! Whether the local user can DM a given wallet, and why not. Mirrors unity-explorer's
//! `PrivateConversationUserStateService.ChatUserState`, resolved in the same order.

use alloy_core::primitives::Address;
use bevy::{ecs::system::SystemParam, prelude::*};
use bevy_console::ConsoleCommand;
use common::{rpc::RpcStreamSender, structs::DmPrivacy, util::AsH160};
use comms::{
    global_crdt::ForeignPlayer,
    livekit::{participant::ParticipantIndex, room::Connected},
    private_chat::{PrivateChatPresenceChanged, PrivateChatPrivacy, PrivateChatRoom},
};
use social::{FriendshipState, SocialClient, SocialStateChanged};
use system_bridge::{DmUserStateData, SystemApi};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DmUserState {
    /// The local user is not in the private chat room (not yet joined, or it failed), so
    /// nothing can be said about anyone else.
    NotConnected,
    /// Friends and other users that both sides allow; the user is in the private chat room.
    Connected,
    /// The local user blocked them.
    BlockedByOwnUser,
    /// The local user only accepts DMs from friends, and they are not one.
    PrivateMessagesBlockedByOwnUser,
    /// They only accept DMs from friends, and the local user is not one.
    PrivateMessagesBlocked,
    /// Offline, or they blocked the local user.
    Disconnected,
    /// In the world through a client without DM support (seen in a realm room but not the
    /// private chat room).
    OtherClient,
}

impl DmUserState {
    fn name(self) -> &'static str {
        match self {
            Self::NotConnected => "notConnected",
            Self::Connected => "connected",
            Self::BlockedByOwnUser => "blockedByOwnUser",
            Self::PrivateMessagesBlockedByOwnUser => "privateMessagesBlockedByOwnUser",
            Self::PrivateMessagesBlocked => "privateMessagesBlocked",
            Self::Disconnected => "disconnected",
            Self::OtherClient => "otherClient",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmUserStatus {
    /// In the private chat room and not blocked by the local user.
    pub online: bool,
    pub state: DmUserState,
}

#[derive(SystemParam)]
pub struct DmUserStates<'w, 's> {
    social: Res<'w, SocialClient>,
    rooms: Query<'w, 's, &'static ParticipantIndex, With<PrivateChatRoom>>,
    connected_rooms: Query<'w, 's, (), (With<PrivateChatRoom>, With<Connected>)>,
    participants: Query<'w, 's, &'static PrivateChatPrivacy>,
    players: Query<'w, 's, &'static ForeignPlayer>,
}

impl DmUserStates<'_, '_> {
    /// Their privacy if they are in the private chat room.
    fn in_private_chat_room(&self, address: Address) -> Option<Option<DmPrivacy>> {
        let index = self.rooms.single().ok()?;
        let entity = index.get(&ParticipantIndex::key(&format!("{address:#x}")))?;
        self.participants.get(*entity).ok().map(|privacy| privacy.0)
    }

    fn in_world(&self, address: Address) -> bool {
        self.players
            .iter()
            .any(|player| player.address == address && !player.transports.is_empty())
    }

    pub fn resolve(&self, address: Address) -> DmUserStatus {
        if self.connected_rooms.is_empty() {
            return DmUserStatus {
                online: false,
                state: DmUserState::NotConnected,
            };
        }
        let (friend, blocked, blocked_by, own_privacy) = match self.social.0.as_ref() {
            Some(client) => (
                self.social.get_state(address) == FriendshipState::Friends,
                client.blocked.contains(&address),
                client.blocked_by.contains(&address),
                client.dm_privacy,
            ),
            None => (false, false, false, DmPrivacy::All),
        };
        let in_room = self.in_private_chat_room(address);
        let online = in_room.is_some() && !blocked;

        let state = if friend {
            if online {
                DmUserState::Connected
            } else {
                DmUserState::Disconnected
            }
        } else if blocked {
            DmUserState::BlockedByOwnUser
        } else if blocked_by {
            // shown as plain offline: being blocked is not disclosed
            return DmUserStatus {
                online: false,
                state: DmUserState::Disconnected,
            };
        } else if in_room.is_none() {
            if self.in_world(address) {
                DmUserState::OtherClient
            } else {
                DmUserState::Disconnected
            }
        } else if own_privacy == DmPrivacy::OnlyFriends {
            DmUserState::PrivateMessagesBlockedByOwnUser
        } else if in_room.is_some_and(|privacy| privacy == Some(DmPrivacy::OnlyFriends)) {
            DmUserState::PrivateMessagesBlocked
        } else {
            DmUserState::Connected
        };
        DmUserStatus { online, state }
    }
}

/// One HUD subscription to a recipient's DM state.
pub struct DmUserStateStream {
    address: Address,
    sender: RpcStreamSender<DmUserStateData>,
    /// What was last sent; `None` until the first emit.
    last: Option<DmUserStatus>,
}

/// Reports each subscribed recipient's DM state to the HUD: once on subscription and then
/// whenever one of its inputs changed and the resolved state with it. Subscriptions end when
/// the HUD drops its end.
#[allow(clippy::too_many_arguments)]
pub fn pipe_dm_user_state_to_scene(
    mut requests: EventReader<SystemApi>,
    mut streams: Local<Vec<DmUserStateStream>>,
    mut social_changed: EventReader<SocialStateChanged>,
    mut presence_changed: EventReader<PrivateChatPresenceChanged>,
    connected: Query<(), (With<PrivateChatRoom>, Added<Connected>)>,
    mut disconnected: RemovedComponents<Connected>,
    mut room_gone: RemovedComponents<PrivateChatRoom>,
    players_changed: Query<&ForeignPlayer, Changed<ForeignPlayer>>,
    mut players_removed: RemovedComponents<ForeignPlayer>,
    states: DmUserStates,
) {
    for request in requests.read() {
        if let SystemApi::GetDmUserStateStream(address, sender) = request {
            match address.as_h160() {
                Some(address) => streams.push(DmUserStateStream {
                    address,
                    sender: sender.clone(),
                    last: None,
                }),
                None => warn!("dm user state stream: `{address}` is not a wallet address"),
            }
        }
    }
    // drain the signals even with no subscribers, so a later one does not see stale ones
    let social_changed = social_changed.read().count() > 0;
    let presence_changed: Vec<Address> = presence_changed.read().map(|e| e.address).collect();
    let room_changed = !connected.is_empty()
        || room_gone.read().count() > 0
        || disconnected
            .read()
            .filter(|room| states.rooms.contains(*room))
            .count()
            > 0;
    let players_removed = players_removed.read().count() > 0;

    streams.retain(|s| !s.sender.is_closed());
    if streams.is_empty() {
        return;
    }
    let inputs_changed = social_changed || room_changed || players_removed;
    let changed_players: Vec<Address> = players_changed.iter().map(|p| p.address).collect();

    for stream in streams.iter_mut() {
        if !(stream.last.is_none()
            || inputs_changed
            || changed_players.contains(&stream.address)
            || presence_changed.contains(&stream.address))
        {
            continue;
        }
        let status = states.resolve(stream.address);
        if stream.last == Some(status) {
            continue;
        }
        stream.last = Some(status);
        let _ = stream.sender.send(DmUserStateData {
            address: format!("{:#x}", stream.address),
            state: status.state.name().to_owned(),
            online: status.online,
        });
    }
}

/// `/dm_state <address>` prints the resolved DM state for a wallet and the inputs behind it.
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/dm_state")]
pub struct DmStateCommand {
    address: String,
}

pub fn dm_state(mut input: ConsoleCommand<DmStateCommand>, states: DmUserStates) {
    let Some(Ok(command)) = input.take() else {
        return;
    };
    let Some(address) = command.address.as_h160() else {
        input.reply_failed(format!("`{}` is not a wallet address", command.address));
        return;
    };
    let status = states.resolve(address);
    let (friendship, blocked, blocked_by, own_privacy) = match states.social.0.as_ref() {
        Some(client) => (
            format!("{:?}", states.social.get_state(address)),
            client.blocked.contains(&address),
            client.blocked_by.contains(&address),
            format!("{:?}", client.dm_privacy),
        ),
        None => (
            "no social client".to_owned(),
            false,
            false,
            "unknown".to_owned(),
        ),
    };
    input.reply_ok(format!(
        "{address:#x}: {:?} (online: {})\n  own room connected: {}\n  friendship: {friendship}\n  blocked: {blocked}, blocked by: {blocked_by}\n  in private chat room: {:?}\n  in world: {}\n  own privacy: {own_privacy}",
        status.state,
        status.online,
        !states.connected_rooms.is_empty(),
        states.in_private_chat_room(address),
        states.in_world(address),
    ));
}

/// True if the local user has blocked `address`; such senders' DMs are dropped on receipt.
pub fn blocked_by_me(social: &SocialClient, address: Address) -> bool {
    social
        .0
        .as_ref()
        .is_some_and(|client| client.blocked.contains(&address))
}
