use std::collections::VecDeque;

use bevy::prelude::*;
use bevy_dui::{DuiCommandsExt, DuiProps, DuiRegistry};
use common::{
    structs::{ShowProfileEvent, SystemAudio},
    util::AsH160,
};
use comms::{chat_marker_things, global_crdt::ChatEvent, profile::UserProfile};
use social::{DirectChatEvent, DirectChatMessage, FriendshipEvent, FriendshipEventBody};
use ui_core::{
    bound_node::{BoundedNode, NodeBounds},
    button::TabManager,
    focus::Focus,
    ui_actions::{Click, EventCloneExt, On},
};

use crate::SystemUiRoot;

use super::{
    conversation_manager::ConversationManager, friends::ShowConversationEvent, ChatInput, ChatTab,
    ChatboxContainer,
};

pub struct ChatHistoryPlugin;

impl Plugin for ChatHistoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter::<ui_core::State>(ui_core::State::Ready),
            setup_chat_history.before(super::setup_chat_popup),
        );
        app.add_systems(Update, update_chat_history);
    }
}

#[derive(Component, Default)]
pub struct ChatHistory {
    current: VecDeque<(Entity, Entity, f32)>,
}

fn setup_chat_history(mut commands: Commands, root: Res<SystemUiRoot>, dui: Res<DuiRegistry>) {
    let history = commands
        .entity(root.0)
        .spawn_template(&dui, "chat-history", DuiProps::new())
        .unwrap();
    commands
        .entity(history.named("chat-content"))
        .insert(ChatHistory::default());
}

#[allow(clippy::too_many_arguments)]
fn update_chat_history(
    mut commands: Commands,
    mut q: Query<(Entity, &mut ChatHistory)>,
    time: Res<Time>,
    mut friends: EventReader<FriendshipEvent>,
    mut private_chats: EventReader<DirectChatEvent>,
    mut nearby_chats: EventReader<ChatEvent>,
    users: Query<&UserProfile>,
    mut pending_friends: Local<Vec<FriendshipEventBody>>,
    mut pending_private_chats: Local<Vec<DirectChatMessage>>,
    mut pending_nearby_chats: Local<Vec<DirectChatMessage>>,
    mut convo: ConversationManager,
    mut node: Query<(&mut NodeBounds, &mut BoundedNode)>,
) {
    pending_friends.extend(friends.read().filter_map(|f| f.0.clone()));
    pending_private_chats.extend(private_chats.read().map(|ev| ev.0.clone()));
    pending_nearby_chats.extend(nearby_chats.read().filter_map(|ev| {
        if ev.channel != "Nearby" {
            return None;
        }

        if chat_marker_things::ALL
            .iter()
            .any(|marker| ev.message.starts_with(*marker))
        {
            return None;
        }

        let partner = if ev.sender == Entity::PLACEHOLDER {
            return None;
        } else {
            let Ok(profile) = users.get(ev.sender) else {
                warn!("can't get profile for chat sender {:?}", ev.sender);
                return None;
            };
            profile.content.eth_address.as_h160()?
        };

        Some(DirectChatMessage {
            partner,
            me_speaking: false,
            message: ev.message.clone(),
        })
    }));

    let Ok((entity, mut history)) = q.single_mut() else {
        return;
    };

    // remove expired
    loop {
        let Some((bubble, message, exp)) = history.current.front() else {
            break;
        };

        // fade the bubbles
        let Ok(mut node) = node.get_mut(*bubble) else {
            warn!("no");
            break;
        };
        let mut alpha = (time.elapsed_secs() - 10.0 - *exp).clamp(-1.0, 0.0) * -0.3;
        if history
            .current
            .get(1)
            .is_some_and(|(next_bubble, ..)| next_bubble == bubble)
        {
            alpha = 0.3;
        }
        node.0.border_color.set_alpha(alpha * 2.0);
        node.1.color.as_mut().unwrap().set_alpha(alpha);

        if *exp > time.elapsed_secs() - 10.0 {
            break;
        }

        // despawn the message
        if let Ok(mut commands) = commands.get_entity(*message) {
            commands.despawn();
        }

        // and the bubble if this was last
        if history
            .current
            .get(1)
            .is_none_or(|(next_bubble, ..)| next_bubble != bubble)
        {
            if let Ok(mut commands) = commands.get_entity(*bubble) {
                commands.despawn();
            }
        }

        history.current.pop_front();
    }

    // add new
    for friend in pending_friends.drain(..) {
        let (message, color, address) = match &friend {
            FriendshipEventBody::Request(r) => (
                "you received a friend request",
                Color::srgb(0.8, 1.0, 1.0),
                &r.user.as_ref().map(|u| &u.address),
            ),
            FriendshipEventBody::Accept(r) => (
                "your friend request was accepted",
                Color::srgb(0.8, 1.0, 1.0),
                &r.user.as_ref().map(|u| &u.address),
            ),
            FriendshipEventBody::Reject(r) => (
                "your friend request was rejected",
                Color::srgb(1.0, 0.8, 0.8),
                &r.user.as_ref().map(|u| &u.address),
            ),
            FriendshipEventBody::Delete(r) => (
                "your friendship is over",
                Color::srgb(1.0, 0.8, 0.8),
                &r.user.as_ref().map(|u| &u.address),
            ),
            FriendshipEventBody::Cancel(r) => (
                "the friend request was cancelled",
                Color::srgb(1.0, 0.8, 0.8),
                &r.user.as_ref().map(|u| &u.address),
            ),
        };

        let Some(address) = address else {
            warn!("no address?");
            continue;
        };
        let Some(h160) = address.as_h160() else {
            warn!("not h160?");
            continue;
        };

        let (bubble, message) =
            convo.add_message(entity, Some(h160), color.with_alpha(0.3), message, false);
        commands.entity(bubble).insert((
            Interaction::default(),
            ShowProfileEvent(h160).send_value_on::<Click>(),
        ));
        history
            .current
            .push_back((bubble, message, time.elapsed_secs()));
    }

    for chat in pending_private_chats.drain(..) {
        if chat.me_speaking {
            continue;
        }

        let (bubble, message) = convo.add_message(
            entity,
            Some(chat.partner),
            Color::srgb(0.8, 1.0, 0.8).with_alpha(0.3),
            chat.message,
            false,
        );
        commands.entity(bubble).insert((
            Interaction::default(),
            ShowConversationEvent(chat.partner).send_value_on::<Click>(),
        ));
        history
            .current
            .push_back((bubble, message, time.elapsed_secs()));
    }

    for chat in pending_nearby_chats.drain(..) {
        let (bubble, message) = convo.add_message(
            entity,
            Some(chat.partner),
            Color::srgb(0.9, 0.9, 0.9).with_alpha(0.3),
            chat.message,
            false,
        );
        commands.entity(bubble).insert((
            Interaction::default(),
            On::<Click>::new(
                |mut commands: Commands,
                 mut container: Query<&mut Node, With<ChatboxContainer>>,
                 entry: Query<Entity, With<ChatInput>>,
                 tab_entity: Query<Entity, With<ChatTab>>,
                 mut tab_mgr: TabManager| {
                    if let Ok(mut style) = container.single_mut() {
                        if style.display == Display::None {
                            commands
                                .send_event(SystemAudio("sounds/ui/toggle_enable.wav".to_owned()));
                            style.display = Display::Flex;
                        };
                    }

                    if let Ok(entry) = entry.single() {
                        commands.entity(entry).insert(Focus);
                    }

                    let Ok(tab_entity) = tab_entity.single() else {
                        warn!("no tab");
                        return;
                    };

                    tab_mgr.set_selected(tab_entity, Some(0));
                },
            ),
        ));
        history
            .current
            .push_back((bubble, message, time.elapsed_secs()));
    }
}
