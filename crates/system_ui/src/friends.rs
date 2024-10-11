use bevy::prelude::*;
use bevy_dui::{DuiCommandsExt, DuiEntities, DuiProps, DuiRegistry};
use common::{
    structs::ShowProfileEvent,
    util::{format_address, AsH160, FireEventEx, TryPushChildrenEx},
};
use comms::profile::ProfileManager;
use dcl_component::proto_components::social::friendship_event_response::{self, Body};
use ethers_core::types::Address;
use ipfs::IpfsAssetServer;
use scene_runner::Toaster;
use social::{client::DirectChatMessage, DirectChatEvent, FriendshipEvent, SocialClient};
use tokio::sync::mpsc::Receiver;
use ui_core::{
    button::{DuiButton, TabManager, TabSelection},
    text_entry::TextEntry,
    ui_actions::{Click, On, UiCaller},
};

use crate::chat::{
    make_chat, ChatBox, ChatInput, ChatTab, ChatboxContainer, DisplayChatMessage,
    PrivateChatEntered,
};

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

// #[derive(Component)]
// pub struct PendingProfileImage(Address);

// pub fn update_profile_images(
//     mut cache: ProfileManager,
//     mut commands: Commands,
//     mut q: Query<(Entity, &PendingProfileImage, &mut Text)>,
// ) {
//     for (ent, pending, mut text) in q.iter_mut() {
//         match cache.get_data(pending.0) {
//             Err(_) => for mut section in &mut text.sections {
//                 section.style.color = Color::srgb(0.5, 0.0, 0.0);
//             }
//             Ok(Some(data)) => {
//                 for mut section in &mut text.sections {
//                     section.style.color = Color::srgb(0.0, 0.0, 0.0);
//                 }
//                 text.sections[1].value = data.name;
//                 commands.entity(ent).remove::<PendingProfileName>();
//             }
//             Ok(None) => (),
//         }
//     }
// }

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
                                .with_prop("profile", On::<Click>::new(move |mut commands: Commands| { commands.fire_event(ShowProfileEvent(friend)); }))
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
                                        }))).unwrap().root;

                                        let button = DuiButton {
                                            enabled: true,
                                            children: Some(button_content),
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
                        .insert(PendingProfileName(friend));
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
                                .with_prop("profile", On::<Click>::new(move |mut commands: Commands| { commands.fire_event(ShowProfileEvent(friend)); }))
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
                        .insert(PendingProfileName(friend));
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
                                .with_prop("profile", On::<Click>::new(move |mut commands: Commands| { commands.fire_event(ShowProfileEvent(friend)); }))
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
                        .insert(PendingProfileName(friend));
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
    mut commands: Commands,
    dui: Res<DuiRegistry>,
    ipfas: IpfsAssetServer,
    client: Res<SocialClient>,
    tab: Query<&TabSelection, With<ChatTab>>,
    mut private_chats: Query<&mut PrivateChat>,
    mut last_chat: Local<Option<Address>>,
    chatbox: Query<Entity, With<ChatBox>>,
    mut text_entry: Query<&mut TextEntry, With<ChatInput>>,
    mut new_chats: EventReader<DirectChatEvent>,
    mut new_chats_outbound: EventReader<PrivateChatEntered>,
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

    let Ok(entity) = chatbox.get_single() else {
        return;
    };

    let make_conv = |commands: &mut Commands, msg: DirectChatMessage| -> Entity {
        make_chat(
            commands,
            ipfas.asset_server(),
            DisplayChatMessage {
                timestamp: 0.0,
                sender: Some(if msg.me_speaking {
                    "me".to_owned()
                } else {
                    "them".to_owned()
                }),
                message: msg.message,
            },
        )
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
        let messages = private_chat
            .messages
            .iter()
            .map(|msg| make_conv(&mut commands, msg.clone()))
            .collect::<Vec<_>>();
        commands.entity(entity).try_push_children(&messages);
    } else {
        // check for new chats
        let new_messages = new_chats
            .iter()
            .filter(|c| c.0.partner == private_chat.address)
            .map(|msg| make_conv(&mut commands, msg.0.clone()))
            .collect::<Vec<_>>();
        commands.entity(entity).try_push_children(&new_messages);
    }

    if private_chat.wants_history_count > 0 {
        if private_chat.history_receiver.is_closed() && private_chat.history_receiver.is_empty() {
            debug!("out of history");
            private_chat.wants_history_count = 0;
        } else {
            while let Ok(history) = private_chat.history_receiver.try_recv() {
                debug!("got history: {:?}", history);
                private_chat.messages.insert(0, history.clone());
                let history = make_conv(&mut commands, history);
                commands.entity(entity).insert_children(0, &[history]);
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
