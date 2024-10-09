use std::sync::Arc;

use bevy::{
    prelude::*,
    tasks::{IoTaskPool, Task},
    utils::hashbrown::HashMap,
};
use bevy_dui::{DuiCommandsExt, DuiEntities, DuiProps, DuiRegistry};
use common::util::{AsH160, FireEventEx, TaskExt, TryPushChildrenEx};
use comms::profile::{get_remote_profile, UserProfile};
use dcl_component::proto_components::social::friendship_event_response::Body;
use ethers_core::types::Address;
use ipfs::{IpfsAssetServer, IpfsIo};
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

#[allow(clippy::too_many_arguments)]
pub fn update_friends(
    mut commands: Commands,
    client: Res<SocialClient>,
    mut init: Local<bool>,
    components: Query<&DuiEntities, With<ChatboxContainer>>,
    dui: Res<DuiRegistry>,
    mut friend_events: EventReader<FriendshipEvent>,
    ipfas: IpfsAssetServer,
    profiles: Query<&UserProfile>,
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
            // TODO don't build this every time
            let names = profiles
                .iter()
                .filter_map(|p| {
                    p.content
                        .eth_address
                        .as_h160()
                        .map(|address| (address, &p.content.name))
                })
                .collect::<HashMap<_, _>>();

            //initialize
            let client = client.0.as_ref().unwrap();
            let new_friends = client
                .friends
                .iter()
                .map(|friend| {
                    let found_name = names.get(friend).map(ToString::to_string);
                    let have_name = found_name.is_some();
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
                                    found_name.unwrap_or_else(|| format!("{friend:#x}")),
                                )
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

                                        let name = me.get(root_id).ok().and_then(|ents| text.get(ents.named("name")).ok()).map(|text| text.sections[0].value.clone()).unwrap_or_else(|| format!("{friend:#x}"));

                                        let short_name = if name.len() > 20 {
                                            format!("{}...", name.chars().take(17).collect::<String>())
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



                                )
                                .with_prop(
                                    "delete",
                                    On::<Click>::new(move |mut client: ResMut<SocialClient>, mut commands: Commands| {
                                        let Some(client) = client.0.as_mut() else {
                                            warn!("no client");
                                            return;
                                        };

                                        let _ = client.delete_friend(friend);
                                        commands.fire_event(FriendshipEvent(None));
                                    }),
                                ),
                        )
                        .unwrap();

                    if !have_name {
                        commands
                            .entity(components.named("name"))
                            .insert(ResolveAddressTask::new(ipfas.ipfs(), friend));
                    }
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
                    let found_name = names.get(friend).map(ToString::to_string);
                    let have_name = found_name.is_some();
                    let friend = *friend;
                    let components = commands
                        .spawn_template(
                            &dui,
                            "sent-pending-friend",
                            DuiProps::default()
                                .with_prop(
                                    "name",
                                    found_name.unwrap_or_else(|| format!("{friend:#x}")),
                                )
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

                    if !have_name {
                        commands
                            .entity(components.named("name"))
                            .insert(ResolveAddressTask::new(ipfas.ipfs(), friend));
                    }
                    components.root
                })
                .collect::<Vec<_>>();
            let new_recd = client
                .received_requests
                .iter()
                .map(|(friend, _msg)| {
                    let found_name = names.get(friend).map(ToString::to_string);
                    let have_name = found_name.is_some();
                    let friend = *friend;
                    let components = commands
                        .spawn_template(
                            &dui,
                            "received-pending-friend",
                            DuiProps::default()
                                .with_prop(
                                    "name",
                                    found_name.unwrap_or_else(|| format!("{friend:#x}")),
                                )
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

                    if !have_name {
                        commands
                            .entity(components.named("name"))
                            .insert(ResolveAddressTask::new(ipfas.ipfs(), friend));
                    }
                    components.root
                })
                .collect::<Vec<_>>();
            let mut pending = commands.entity(components.named("pending-friends"));
            pending.despawn_descendants();
            pending.try_push_children(&new_sent);
            pending.try_push_children(&new_recd);
        }
    }
}

#[derive(Component)]
pub struct ResolveAddressTask(Task<Result<String, anyhow::Error>>);

impl ResolveAddressTask {
    fn new(ipfs: &Arc<IpfsIo>, address: Address) -> Self {
        let ipfs = Arc::clone(ipfs);
        let task = IoTaskPool::get().spawn(async move {
            debug!("resolve address: starting profile fetch");
            let str_address = format!("{address:x}");
            let str_address = str_address
                .chars()
                .skip(str_address.len().saturating_sub(4))
                .collect::<String>();
            let res = get_remote_profile(address, ipfs)
                .await
                .map(|profile| format!("{}#{}", profile.content.name, str_address));
            debug!("resolve address: ending profile fetch: {:?}", res);
            res
        });
        Self(task)
    }
}

pub fn resolve_addresses(
    mut commands: Commands,
    mut q: Query<(Entity, &mut ResolveAddressTask, &mut Text)>,
) {
    for (ent, mut task, mut text) in q.iter_mut() {
        if let Some(name) = task.0.complete() {
            match name {
                Ok(name) => text.sections[0].value = name,
                Err(e) => warn!("failed to resolve user name: {e}"),
            };
            commands.entity(ent).remove::<ResolveAddressTask>();
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
    mut tasks: Local<Vec<(String, Task<String>)>>,
    mut ix: Local<usize>,
    profiles: Query<&UserProfile>,
    ipfas: IpfsAssetServer,
) {
    if friends.is_empty() && chats.is_empty() && tasks.is_empty() {
        return;
    }

    // TODO don't build this every time
    let names = profiles
        .iter()
        .filter_map(|p| {
            p.content
                .eth_address
                .as_h160()
                .map(|address| (address, &p.content.name))
        })
        .collect::<HashMap<_, _>>();

    for friend in friends.read() {
        if let Some(body) = friend.0.as_ref() {
            let (message, address) = match body {
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

            let user_task = if let Some(h160) = address.and_then(AsH160::as_h160) {
                if let Some(name) = names.get(&h160).map(ToString::to_string) {
                    let name = name.clone();
                    IoTaskPool::get().spawn(async move { name })
                } else {
                    let task = ResolveAddressTask::new(ipfas.ipfs(), h160);
                    let address = address.cloned();
                    IoTaskPool::get().spawn(async move {
                        let resolved = task.0.await;
                        resolved.unwrap_or(address.unwrap())
                    })
                }
            } else {
                let address = address.cloned();
                IoTaskPool::get().spawn(async move { address.unwrap_or_default() })
            };
            tasks.push((message.to_string(), user_task));
        }
    }

    for chat in chats.read().filter(|&chat| !chat.0.me_speaking) {
        let user_task = if let Some(name) = names.get(&chat.0.partner).map(ToString::to_string) {
            let name = name.clone();
            IoTaskPool::get().spawn(async move { name })
        } else {
            let task = ResolveAddressTask::new(ipfas.ipfs(), chat.0.partner);
            let address = chat.0.partner;
            IoTaskPool::get().spawn(async move {
                let resolved = task.0.await;
                resolved.unwrap_or_else(|_| format!("{address:#x}"))
            })
        };

        tasks.push((chat.0.message.clone(), user_task));
    }

    tasks.retain_mut(|(msg, task)| {
        if let Some(name) = task.complete() {
            toaster.add_toast(format!("friendy {}", *ix), format!("{}: {}", name, msg));
            *ix += 1;
            false
        } else {
            true
        }
    });
}
