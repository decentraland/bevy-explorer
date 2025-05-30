use bevy::{prelude::*, utils::hashbrown::HashMap};
use bevy_dui::{DuiCommandsExt, DuiEntities, DuiProps, DuiRegistry};
use common::{
    structs::{ShowProfileEvent, SystemAudio},
    util::{format_address, FireEventEx, TryPushChildrenEx},
};
use comms::profile::ProfileManager;
use ethers_core::types::Address;
use social::{DirectChatEvent, DirectChatMessage, FriendshipEvent, SocialClient};
use tokio::sync::mpsc::Receiver;
use ui_core::{
    button::{DuiButton, TabManager, TabSelection},
    focus::Focus,
    text_entry::TextEntry,
    ui_actions::{Click, EventCloneExt, On},
    user_font, FontName, WeightName,
};

use crate::chat::{ChatInput, ChatTab, ChatboxContainer, PrivateChatEntered};

use super::{conversation_manager::ConversationManager, ChatBox};

pub struct FriendsPlugin;

impl Plugin for FriendsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                update_friends,
                update_conversations,
                show_conversation,
                update_profile_names,
                update_profile_images,
                bold_unread,
            ),
        );
        app.add_event::<ShowConversationEvent>();
    }
}

#[derive(Component)]
pub struct PrivateChat {
    pub address: Address,
    pub history_receiver: Receiver<DirectChatMessage>,
    pub wants_history_count: usize,
    pub messages: Vec<DirectChatMessage>,
}

#[derive(Component)]
pub struct PendingProfileName(Address);

pub fn update_profile_names(
    mut cache: ProfileManager,
    mut commands: Commands,
    mut q: Query<(Entity, &PendingProfileName, &mut Text)>,
) {
    for (ent, pending, mut text) in q.iter_mut() {
        match cache.get_name(pending.0) {
            Err(_) => {
                for section in &mut text.sections {
                    section.style.color = Color::srgb(0.5, 0.0, 0.0);
                }
                commands.entity(ent).remove::<PendingProfileName>();
            }
            Ok(Some(name)) => {
                for section in &mut text.sections {
                    section.style.color = Color::srgb(0.0, 0.0, 0.0);
                    if section.value.starts_with("0x") {
                        section.value = format_address(pending.0, Some(name));
                    }
                }
                commands.entity(ent).remove::<PendingProfileName>();
            }
            Ok(None) => (),
        }
    }
}

#[derive(Component)]
pub struct PendingProfileUiImage(pub Address);

pub fn update_profile_images(
    mut commands: Commands,
    mut cache: ProfileManager,
    mut q: Query<(Entity, &PendingProfileUiImage, &mut UiImage)>,
) {
    for (ent, pending, mut ui_image) in q.iter_mut() {
        match cache.get_image(pending.0) {
            Err(_) => {
                commands.entity(ent).remove::<PendingProfileName>();
            }
            Ok(Some(image)) => {
                ui_image.texture = image;
                commands.entity(ent).remove::<PendingProfileUiImage>();
            }
            Ok(None) => (),
        }
    }
}

#[derive(Event, Clone)]
pub struct ShowConversationEvent(pub Address);

#[allow(clippy::too_many_arguments)]
pub fn show_conversation(
    mut show_events: EventReader<ShowConversationEvent>,
    mut pending_event: Local<Option<Address>>,
    mut commands: Commands,
    client: Res<SocialClient>,
    dui: Res<DuiRegistry>,
    existing_chats: Query<(Entity, &PrivateChat)>,
    mut tab_manager: TabManager,
    tab: Query<Entity, With<ChatTab>>,
    mut profile_cache: ProfileManager,
    mut container: Query<&mut Style, With<ChatboxContainer>>,
    entry: Query<Entity, With<ChatInput>>,
) {
    if let Some(event) = show_events.read().last() {
        *pending_event = Some(event.0);
    }

    let Ok(tab) = tab.get_single() else {
        return;
    };

    let Some(&friend) = pending_event.as_ref() else {
        return;
    };

    let name = match profile_cache.get_name(friend) {
        Ok(Some(name)) => name.to_owned(),
        Ok(None) => return,
        Err(_) => format!("{friend:#x}"),
    };

    // we're going ahead after these checks, so clear the pending
    pending_event.take();

    if let Ok(mut style) = container.get_single_mut() {
        if style.display == Display::None {
            commands.fire_event(SystemAudio("sounds/ui/toggle_enable.wav".to_owned()));
            style.display = Display::Flex;
        };
    }

    if let Ok(entry) = entry.get_single() {
        commands.entity(entry).insert(Focus);
    }

    if let Some((existing, _)) = existing_chats.iter().find(|(_, c)| c.address == friend) {
        tab_manager.set_selected_entity(tab, existing);
        return;
    }

    let short_name = if name.len() > 25 {
        format!(
            "{}...{}",
            name.chars().take(17).collect::<String>(),
            name.chars().skip(name.len() - 8).collect::<String>()
        )
    } else {
        name
    };

    let button_content = commands
        .spawn_template(
            &dui,
            "direct-chat-button",
            DuiProps::default().with_prop("name", short_name).with_prop(
                "close",
                On::<Click>::new(
                    move |mut tab_manager: TabManager,
                          tab: Query<Entity, With<ChatTab>>,
                          buttons: Query<(Entity, &PrivateChat)>| {
                        // delete this tab
                        let Ok(tab) = tab.get_single() else {
                            return;
                        };

                        let Some((this, _)) = buttons.iter().find(|(_, b)| b.address == friend)
                        else {
                            return;
                        };

                        tab_manager.remove_entity(tab, this);
                    },
                ),
            ),
        )
        .unwrap();

    commands
        .entity(button_content.named("name"))
        .insert(BoldUnread(friend));

    let button = DuiButton {
        enabled: true,
        children: Some(button_content.root),
        ..Default::default()
    };

    let new_tab = tab_manager
        .add(
            tab,
            None,
            button,
            false,
            Some(UiRect::new(
                Val::Px(1.0),
                Val::Px(1.0),
                Val::Px(1.0),
                Val::Px(0.0),
            )),
        )
        .unwrap()
        .root;

    let Some(client) = client.0.as_ref() else {
        warn!("social not connected");
        return;
    };

    let Ok(history_receiver) = client.get_chat_history(friend) else {
        warn!("failed to get history");
        return;
    };

    commands.entity(new_tab).insert(PrivateChat {
        address: friend,
        history_receiver,
        wants_history_count: 10,
        messages: Vec::default(),
    });

    tab_manager.set_selected_entity(tab, new_tab);
}

#[allow(clippy::too_many_arguments)]
pub fn update_friends(
    mut commands: Commands,
    client: Res<SocialClient>,
    mut init: Local<bool>,
    components: Query<&DuiEntities, With<ChatboxContainer>>,
    dui: Res<DuiRegistry>,
    mut friend_events: EventReader<FriendshipEvent>,
) {
    let is_init = client.0.as_ref().is_some_and(|c| c.is_initialized);
    if is_init != *init || friend_events.read().next().is_some() {
        *init = is_init;
        let Ok(components) = components.get_single() else {
            return;
        };
        if !is_init {
            // clean up, disconnected
        } else {
            //initialize
            let client = client.0.as_ref().unwrap();
            let new_friends = client
                .friends
                .iter()
                .map(|friend| {
                    let friend = *friend;
                    let mut root = commands.spawn_empty();
                    let components = dui
                        .apply_template(
                            &mut root,
                            "friend",
                            DuiProps::default()
                                .with_prop(
                                    "name",
                                    format!("<b>{}</b>", format_address(friend, None)),
                                )
                                .with_prop(
                                    "profile",
                                    ShowProfileEvent(friend).send_value_on::<Click>(),
                                )
                                .with_prop(
                                    "chat",
                                    ShowConversationEvent(friend).send_value_on::<Click>(),
                                ),
                        )
                        .unwrap();

                    commands
                        .entity(components.named("name"))
                        .insert(PendingProfileName(friend))
                        .insert(BoldUnread(friend));
                    components.root
                })
                .collect::<Vec<_>>();
            let mut friends = commands.entity(components.named("friends"));
            friends.despawn_descendants();
            friends.try_push_children(&new_friends);

            let new_sent = client
                .sent_requests
                .iter()
                .map(|friend| {
                    let friend = *friend;
                    let components = commands
                        .spawn_template(
                            &dui,
                            "sent-pending-friend",
                            DuiProps::default()
                                .with_prop(
                                    "name",
                                    format!("<b>{}</b>", format_address(friend, None)),
                                )
                                .with_prop("profile", ShowProfileEvent(friend).send_value_on::<Click>())
                                .with_prop(
                                    "cancel",
                                    On::<Click>::new(move |mut client: ResMut<SocialClient>, mut commands: Commands| {
                                        let Some(client) = client.0.as_mut() else {
                                            warn!("no client");
                                            return;
                                        };

                                        let _ = client.cancel_request(friend);
                                        commands.fire_event(FriendshipEvent(None));
                                    }),
                                ),
                        )
                        .unwrap();

                    commands
                        .entity(components.named("name"))
                        .insert(PendingProfileName(friend))
                        .insert(BoldUnread(friend));
                    components.root
                })
                .collect::<Vec<_>>();

            let mut sent_pending = commands.entity(components.named("sent-friends"));
            sent_pending.despawn_descendants();
            sent_pending.try_push_children(&new_sent);

            let new_recd = client
                .received_requests
                .iter()
                .map(|(friend, _msg)| {
                    let friend = *friend;
                    let components = commands
                        .spawn_template(
                            &dui,
                            "received-pending-friend",
                            DuiProps::default()
                                .with_prop(
                                    "name",
                                    format!("<b>{}</b>", format_address(friend, None)),
                                )
                                .with_prop("profile", ShowProfileEvent(friend).send_value_on::<Click>())
                                .with_prop(
                                    "accept",
                                    On::<Click>::new(move |mut client: ResMut<SocialClient>, mut commands: Commands| {
                                        let Some(client) = client.0.as_mut() else {
                                            warn!("no client");
                                            return;
                                        };

                                        let _ = client.accept_request(friend);
                                        commands.fire_event(FriendshipEvent(None));
                                    }),
                                )
                                .with_prop(
                                    "reject",
                                    On::<Click>::new(move |mut client: ResMut<SocialClient>, mut commands: Commands| {
                                        let Some(client) = client.0.as_mut() else {
                                            warn!("no client");
                                            return;
                                        };

                                        let _ = client.reject_request(friend);
                                        commands.fire_event(FriendshipEvent(None));
                                    }),
                                ),
                        )
                        .unwrap();

                    commands
                        .entity(components.named("name"))
                        .insert(PendingProfileName(friend))
                        .insert(BoldUnread(friend));
                    components.root
                })
                .collect::<Vec<_>>();

            let mut recd_pending = commands.entity(components.named("received-friends"));
            recd_pending.despawn_descendants();
            recd_pending.try_push_children(&new_recd);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn update_conversations(
    mut client: ResMut<SocialClient>,
    tab: Query<&TabSelection, With<ChatTab>>,
    chatbox: Query<Entity, With<ChatBox>>,
    mut private_chats: Query<&mut PrivateChat>,
    mut last_chat: Local<Option<Address>>,
    mut text_entry: Query<&mut TextEntry, With<ChatInput>>,
    mut new_chats: EventReader<DirectChatEvent>,
    mut new_chats_outbound: EventReader<PrivateChatEntered>,
    mut conversation: ConversationManager,
) {
    let (Ok(tab), Ok(chatbox)) = (tab.get_single(), chatbox.get_single()) else {
        return;
    };

    let new_chats = new_chats.read().collect::<Vec<_>>();

    if !new_chats.is_empty() {
        for mut private_chat in private_chats.iter_mut() {
            let address = private_chat.address;
            for chat in new_chats.iter().filter(|c| c.0.partner == address) {
                private_chat.messages.push(chat.0.clone());
            }
        }
    }

    let Some(private_chat_ent) = tab.selected_entity() else {
        *last_chat = None;
        return;
    };
    let private_chat_ent = private_chat_ent.root;
    let Ok(mut private_chat) = private_chats.get_mut(private_chat_ent) else {
        *last_chat = None;
        return;
    };

    if let Some(client) = client.0.as_mut() {
        client.mark_as_read(private_chat.address);
    }

    if *last_chat != Some(private_chat.address) {
        // init
        *last_chat = Some(private_chat.address);

        conversation.clear(chatbox);
        text_entry.single_mut().enabled = true;

        if private_chat.wants_history_count == 0
            && !(private_chat.history_receiver.is_closed()
                && private_chat.history_receiver.is_empty())
        {
            // add button
            conversation.add_history_button(chatbox, private_chat_ent);
        }

        // add current messages
        for message in &private_chat.messages {
            debug!("make conv");
            if message.me_speaking {
                conversation.add_message(
                    chatbox,
                    None,
                    Color::srgb(0.8, 0.8, 1.0),
                    &message.message,
                    false,
                );
            } else {
                conversation.add_message(
                    chatbox,
                    Some(message.partner),
                    Color::srgb(0.8, 1.0, 0.8),
                    &message.message,
                    false,
                );
            }
        }
    } else {
        // check for new chats
        for new_message in new_chats
            .iter()
            .filter(|c| c.0.partner == private_chat.address)
        {
            if new_message.0.me_speaking {
                conversation.add_message(
                    chatbox,
                    None,
                    Color::srgb(0.8, 0.8, 1.0),
                    &new_message.0.message,
                    false,
                );
            } else {
                conversation.add_message(
                    chatbox,
                    Some(new_message.0.partner),
                    Color::srgb(0.8, 1.0, 0.8),
                    &new_message.0.message,
                    false,
                );
            }
        }
    }

    if private_chat.wants_history_count > 0 {
        if private_chat.history_receiver.is_closed() && private_chat.history_receiver.is_empty() {
            debug!("out of history");
            private_chat.wants_history_count = 0;
        } else {
            while let Ok(history) = private_chat.history_receiver.try_recv() {
                debug!("got history: {:?}", history);
                private_chat.messages.insert(0, history.clone());
                if history.me_speaking {
                    conversation.add_message(
                        chatbox,
                        None,
                        Color::srgb(0.8, 0.8, 1.0),
                        &history.message,
                        true,
                    );
                } else {
                    conversation.add_message(
                        chatbox,
                        Some(history.partner),
                        Color::srgb(0.8, 1.0, 0.8),
                        &history.message,
                        true,
                    );
                }
                private_chat.wants_history_count -= 1;
                if private_chat.wants_history_count == 0 {
                    // add button
                    conversation.add_history_button(chatbox, private_chat_ent);
                    break;
                }
            }
        }
    }

    for chat in new_chats_outbound.read() {
        if let Some(client) = client.0.as_ref() {
            client.chat(private_chat.address, chat.0.clone()).unwrap();
        }
    }
}

#[derive(Component)]
pub struct BoldUnread(Address);

pub fn bold_unread(mut q: Query<(&mut Text, Ref<BoldUnread>)>, client: Res<SocialClient>) {
    let default = HashMap::default();
    let unread = client
        .0
        .as_ref()
        .map(|client| client.unread_messages())
        .unwrap_or(&default);
    for (mut text, b) in q.iter_mut() {
        let bold = unread.get(&b.0).copied().unwrap_or(0) > 0;
        for section in &mut text.sections {
            section.style.font = user_font(
                FontName::Sans,
                if bold {
                    WeightName::Bold
                } else {
                    WeightName::Regular
                },
            );
        }
    }
}
