//! Whether the local user can DM a given wallet, and why not. Mirrors unity-explorer's
//! `PrivateConversationUserStateService.ChatUserState`, resolved in the same order.

use alloy_core::primitives::Address;
use bevy::{ecs::system::SystemParam, prelude::*};
use bevy_console::ConsoleCommand;
use common::{structs::DmPrivacy, util::AsH160};
use comms::{
    global_crdt::ForeignPlayer,
    livekit::participant::{HostingParticipants, LivekitParticipant},
    private_chat::{PrivateChatPrivacy, PrivateChatRoom},
};
use social::{FriendshipState, SocialClient};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DmUserState {
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

#[derive(Clone, Copy, Debug)]
pub struct DmUserStatus {
    /// In the private chat room and not blocked by the local user.
    pub online: bool,
    pub state: DmUserState,
}

#[derive(SystemParam)]
pub struct DmUserStates<'w, 's> {
    social: Res<'w, SocialClient>,
    rooms: Query<'w, 's, &'static HostingParticipants, With<PrivateChatRoom>>,
    participants: Query<'w, 's, (&'static LivekitParticipant, &'static PrivateChatPrivacy)>,
    players: Query<'w, 's, &'static ForeignPlayer>,
}

impl DmUserStates<'_, '_> {
    /// Their privacy if they are in the private chat room.
    fn in_private_chat_room(&self, address: Address) -> Option<Option<DmPrivacy>> {
        let hosting = self.rooms.single().ok()?;
        self.participants
            .iter_many(hosting.collection())
            .find(|(participant, _)| participant.identity().as_str().as_h160() == Some(address))
            .map(|(_, privacy)| privacy.0)
    }

    fn in_world(&self, address: Address) -> bool {
        self.players
            .iter()
            .any(|player| player.address == address && !player.transports.is_empty())
    }

    pub fn resolve(&self, address: Address) -> DmUserStatus {
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
        "{address:#x}: {:?} (online: {})\n  friendship: {friendship}\n  blocked: {blocked}, blocked by: {blocked_by}\n  in private chat room: {:?}\n  in world: {}\n  own privacy: {own_privacy}",
        status.state,
        status.online,
        states.in_private_chat_room(address),
        states.in_world(address),
    ));
}
