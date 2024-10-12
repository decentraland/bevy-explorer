use bevy::{core::FrameCount, prelude::*, utils::hashbrown::HashMap};
use bevy_dui::{DuiCommandsExt, DuiEntities, DuiProps, DuiRegistry};
use common::{
    structs::ShowProfileEvent,
    util::{format_address, AsH160, FireEventEx, TryPushChildrenEx},
};
use comms::profile::ProfileManager;
use copypasta::{ClipboardContext, ClipboardProvider};
use dcl_component::proto_components::social::friendship_event_response::{self, Body};
use ethers_core::types::Address;
use scene_runner::Toaster;
use social::{client::DirectChatMessage, DirectChatEvent, FriendshipEvent, SocialClient};
use tokio::sync::mpsc::Receiver;
use ui_core::{
    button::{DuiButton, TabManager, TabSelection},
    text_entry::TextEntry,
    ui_actions::{Click, EventCloneExt, On, UiCaller},
    user_font, FontName, WeightName,
};
use wallet::Wallet;

use crate::chat::{ChatBox, ChatInput, ChatTab, ChatboxContainer, PrivateChatEntered};

pub struct FriendsPlugin;

impl Plugin for FriendsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                update_friends,
                update_conversations,
                show_popups,
                update_profile_names,
                update_profile_images,
                bold_unread,
            ),
        );
    }
}

#[derive(Component)]
pub struct PrivateChat {
    address: Address,
    history_receiver: Receiver<DirectChatMessage>,
    wants_history_count: usize,
    messages: Vec<DirectChatMessage>,
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
pub struct PendingProfileUiImage(Address);

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

#[allow(clippy::too_many_arguments)]
pub fn update_friends(
    mut commands: Commands,
    client: Res<SocialClient>,
    mut init: Local<bool>,
    components: Query<&DuiEntities, With<ChatboxContainer>>,
    dui: Res<DuiRegistry>,
    mut friend_events: EventReader<FriendshipEvent>,
) {
    let is_init = client.0.as_ref().map_or(false, |c| c.is_initialized);
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
                    let root_id = root.id();
                    let components = dui
                        .apply_template(
                            &mut root,
                            "friend",
                            DuiProps::default()
                                .with_prop(
                                    "name",
                                    format!("<b>{}</b>", format_address(friend, None)),
                                )
                                .with_prop("profile", ShowProfileEvent(friend).send_value_on::<Click>())
                                .with_prop(
                                    "chat",

                                    On::<Click>::new(move |
                                        mut commands: Commands,
                                        client: Res<SocialClient>,
                                        dui: Res<DuiRegistry>,
                                        existing_chats: Query<(Entity, &PrivateChat)>,
                                        mut tab_manager: TabManager,
                                        tab: Query<Entity, With<ChatTab>>,
                                        me: Query<&DuiEntities>,
                                        text: Query<&Text>,
                                    | {
                                        let Ok(tab) = tab.get_single() else {
                                            return;
                                        };

                                        if let Some((existing, _)) = existing_chats.iter().find(|(_, c)| c.address == friend) {
                                            tab_manager.set_selected_entity(tab, existing);
                                            return;
                                        }

                                        let name = me.get(root_id).ok().and_then(|ents| text.get(ents.named("name")).ok()).map(|text| text.sections[1].value.clone()).unwrap_or_else(|| format_address(friend, None));

                                        let short_name = if name.len() > 25 {
                                            format!("{}...{}", name.chars().take(17).collect::<String>(), name.chars().skip(name.len() - 8).collect::<String>())
                                        } else {
                                            name
                                        };

                                        let button_content = commands.spawn_template(&dui, "direct-chat-button", DuiProps::default().with_prop("name", short_name).with_prop("close", On::<Click>::new(move |
                                            mut tab_manager: TabManager,
                                            tab: Query<Entity, With<ChatTab>>,
                                            buttons: Query<(Entity, &PrivateChat)>,
                                        | {
                                            // delete this tab
                                            let Ok(tab) = tab.get_single() else {
                                                return;
                                            };

                                            let Some((this, _)) = buttons.iter().find(|(_, b)| b.address == friend) else {
                                                return;
                                            };

                                            tab_manager.remove_entity(tab, this);
                                        }))).unwrap();

                                        commands.entity(button_content.named("name")).insert(BoldUnread(friend));

                                        let button = DuiButton {
                                            enabled: true,
                                            children: Some(button_content.root),
                                            ..Default::default()
                                        };

                                        let new_tab = tab_manager.add(
                                            tab,
                                            None,
                                            button,
                                            false,
                                            Some(UiRect::new(Val::Px(1.0), Val::Px(1.0), Val::Px(1.0), Val::Px(0.0))),
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

#[derive(Component)]
pub struct ChatContainer(pub Option<Address>);

#[allow(clippy::too_many_arguments)]
pub fn update_conversations(
    mut commands: Commands,
    dui: Res<DuiRegistry>,
    mut client: ResMut<SocialClient>,
    tab: Query<&TabSelection, With<ChatTab>>,
    mut private_chats: Query<&mut PrivateChat>,
    mut last_chat: Local<Option<Address>>,
    chatbox: Query<(Entity, Option<&Children>), With<ChatBox>>,
    mut text_entry: Query<&mut TextEntry, With<ChatInput>>,
    mut new_chats: EventReader<DirectChatEvent>,
    mut new_chats_outbound: EventReader<PrivateChatEntered>,
    containers: Query<(&ChatContainer, &DuiEntities)>,
    wallet: Res<Wallet>,
) {
    let Ok(tab) = tab.get_single() else {
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

    let Ok((entity, children)) = chatbox.get_single() else {
        return;
    };
    let mut containers = children
        .map(|c| c.iter().copied())
        .unwrap_or_default()
        .flat_map(|c| {
            containers
                .get(c)
                .ok()
                .map(|(c, ents)| (c.0, ents.named("content")))
        })
        .collect::<Vec<_>>();

    let mut get_container =
        |commands: &mut Commands, address: Option<Address>, historic: bool| -> Entity {
            let potential_container = if historic {
                containers.first()
            } else {
                containers.last()
            };

            if let Some((existing_address, content)) = potential_container {
                if *existing_address == address {
                    debug!("{:?} -> existing {}", address, *content);
                    return *content;
                }
            }

            let components = if let Some(address) = address {
                let components = commands
                    .spawn_template(&dui, "other-chat-container", DuiProps::new())
                    .unwrap();
                commands
                    .entity(components.named("image"))
                    .insert(ShowProfileEvent(address).send_value_on::<Click>());
                components
            } else {
                commands
                    .spawn_template(&dui, "me-chat-container", DuiProps::default())
                    .unwrap()
            };
            if let Some(address) = address.or_else(|| wallet.address()) {
                commands
                    .entity(components.named("image"))
                    .insert(PendingProfileUiImage(address));
            }
            commands
                .entity(components.root)
                .insert(ChatContainer(address));

            let content = components.named("content");

            if historic {
                commands
                    .entity(entity)
                    .insert_children(0, &[components.root]);
                containers.insert(0, (address, content));
            } else {
                commands.entity(entity).push_children(&[components.root]);
                containers.push((address, content));
            }

            debug!("{:?} -> new {}", address, content);
            content
        };

    let mut make_conv = |commands: &mut Commands, message: DirectChatMessage, historic: bool| {
        let container = get_container(
            commands,
            (!message.me_speaking).then_some(message.partner),
            historic,
        );
        debug!("container: {container:?}");

        let message_copy = message.message.clone();
        let message = commands
            .spawn_template(
                &dui,
                if message.me_speaking {
                    "chat-content-me"
                } else {
                    "chat-content-other"
                },
                DuiProps::new()
                    .with_prop("text", message.message.clone())
                    .with_prop(
                        "copy",
                        On::<Click>::new(move |mut toaster: Toaster, frame: Res<FrameCount>| {
                            let Ok(mut ctx) = ClipboardContext::new() else {
                                warn!("failed to copy");
                                return;
                            };

                            if ctx.set_contents(message_copy.clone()).is_ok() {
                                toaster.add_toast(
                                    format!("chatcopy {}", frame.0),
                                    "Message copied to clipboard",
                                );
                            } else {
                                toaster.add_toast(
                                    format!("chatcopy {}", frame.0),
                                    "Failed to copy message",
                                );
                            }
                        }),
                    ),
            )
            .unwrap()
            .root;
        commands.entity(container).try_push_children(&[message]);
        debug!("added");
    };

    let make_history_button = |commands: &mut Commands, private_chat_ent: Entity| {
        commands
            .spawn_template(
                &dui,
                "button",
                DuiProps::new()
                    .with_prop("label", "load more history".to_string())
                    .with_prop(
                        "onclick",
                        On::<Click>::new(
                            move |mut chat: Query<&mut PrivateChat>,
                                  caller: Res<UiCaller>,
                                  parent: Query<&Parent>,
                                  mut commands: Commands| {
                                if let Ok(parent) = parent.get(caller.0) {
                                    commands.entity(parent.get()).despawn_recursive();
                                }
                                if let Ok(mut chat) = chat.get_mut(private_chat_ent) {
                                    chat.wants_history_count = 10;
                                }
                            },
                        ),
                    ),
            )
            .unwrap()
            .root
    };

    if *last_chat != Some(private_chat.address) {
        // init
        *last_chat = Some(private_chat.address);

        commands.entity(entity).despawn_descendants();
        text_entry.single_mut().enabled = true;

        if private_chat.wants_history_count == 0
            && !(private_chat.history_receiver.is_closed()
                && private_chat.history_receiver.is_empty())
        {
            // add button
            let button = make_history_button(&mut commands, private_chat_ent);
            commands.entity(entity).insert_children(0, &[button]);
        }

        // add current messages
        for message in &private_chat.messages {
            debug!("make conv");
            make_conv(&mut commands, message.clone(), false);
        }
    } else {
        // check for new chats
        for new_message in new_chats
            .iter()
            .filter(|c| c.0.partner == private_chat.address)
        {
            make_conv(&mut commands, new_message.0.clone(), false);
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
                make_conv(&mut commands, history, true);
                private_chat.wants_history_count -= 1;
                if private_chat.wants_history_count == 0 {
                    // add button
                    let button = make_history_button(&mut commands, private_chat_ent);
                    commands.entity(entity).insert_children(0, &[button]);
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

pub fn show_popups(
    mut toaster: Toaster,
    mut friends: EventReader<FriendshipEvent>,
    mut chats: EventReader<DirectChatEvent>,
    mut ix: Local<usize>,
    mut pending_friends: Local<Vec<friendship_event_response::Body>>,
    mut pending_chats: Local<Vec<DirectChatMessage>>,
    mut cache: ProfileManager,
) {
    pending_friends.extend(friends.read().filter_map(|f| f.0.clone()));
    pending_chats.extend(chats.read().map(|e| e.0.clone()));

    *pending_friends = pending_friends
        .drain(..)
        .filter_map(|friend| {
            let (message, address) = match &friend {
                Body::Request(r) => (
                    "you received a friend request",
                    &r.user.as_ref().map(|u| &u.address),
                ),
                Body::Accept(r) => (
                    "your friend request was accepted",
                    &r.user.as_ref().map(|u| &u.address),
                ),
                Body::Reject(r) => (
                    "your friend request was rejected",
                    &r.user.as_ref().map(|u| &u.address),
                ),
                Body::Delete(r) => (
                    "your friendship is over",
                    &r.user.as_ref().map(|u| &u.address),
                ),
                Body::Cancel(r) => (
                    "the friend request was cancelled",
                    &r.user.as_ref().map(|u| &u.address),
                ),
            };

            let Some(address) = address else {
                warn!("no address?");
                return None;
            };
            let Some(h160) = address.as_h160() else {
                warn!("not h160?");
                return None;
            };

            let name = match cache.get_name(h160) {
                Ok(None) => return Some(friend),
                Ok(Some(name)) => name.to_owned(),
                Err(_) => address.to_string(),
            };

            toaster.add_toast(format!("friendy {}", *ix), format!("{}: {}", name, message));
            *ix += 1;
            None
        })
        .collect();

    *pending_chats = pending_chats
        .drain(..)
        .filter_map(|chat| {
            if chat.me_speaking {
                return None;
            }

            let name = match cache.get_name(chat.partner) {
                Ok(None) => return Some(chat),
                Ok(Some(name)) => format_address(chat.partner, Some(name)),
                Err(_) => format_address(chat.partner, None),
            };

            toaster.add_toast(
                format!("friendy {}", *ix),
                format!("{}: {}", name, chat.message),
            );
            *ix += 1;
            None
        })
        .collect();
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
