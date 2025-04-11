pub mod conversation_manager;
pub mod friends;
pub mod history;

use bevy::{color::palettes::css, prelude::*};

use bevy_console::{ConsoleCommand, ConsoleCommandEntered, ConsoleConfiguration, PrintConsoleLine};
use bevy_dui::{DuiCommandsExt, DuiEntities, DuiProps, DuiRegistry};
use common::{
    dcl_assert,
    inputs::SystemAction,
    sets::SetupSets,
    structs::{PrimaryPlayerRes, PrimaryUser, SystemAudio, ToolTips, TooltipSource},
    util::{
        AsH160, FireEventEx, ModifyComponentExt, RingBuffer, RingBufferReceiver, TryPushChildrenEx,
    },
};
use comms::{
    chat_marker_things,
    global_crdt::{ChatEvent, ForeignPlayer},
    profile::UserProfile,
    NetworkMessage, Transport,
};
use console::DoAddConsoleCommand;
use conversation_manager::ConversationManager;
use dcl::{SceneLogLevel, SceneLogMessage};
use dcl_component::proto_components::kernel::comms::rfc4;
use ethers_core::types::Address;
use history::ChatHistoryPlugin;
use input_manager::{InputManager, InputPriority};
use scene_runner::{renderer_context::RendererSceneContext, ContainingScene};
use shlex::Shlex;
use social::FriendshipEvent;
use system_bridge::{ChatMessage, SystemApi};
use ui_core::{
    button::{DuiButton, TabSelection},
    focus::Focus,
    text_entry::{TextEntry, TextEntrySubmit},
    text_size::FontSize,
    ui_actions::{Click, DataChanged, HoverEnter, HoverExit, On},
};

use friends::FriendsPlugin;
use wallet::Wallet;

use super::SystemUiRoot;

pub struct ChatPanelPlugin;

impl Plugin for ChatPanelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, display_chat);
        app.add_systems(Update, append_chat_messages);
        app.add_systems(Update, emit_user_chat);
        app.add_systems(Update, (pipe_chats_to_scene, pipe_chats_from_scene));
        app.add_systems(Startup, setup.in_set(SetupSets::Main));
        app.add_systems(
            OnEnter::<ui_core::State>(ui_core::State::Ready),
            setup_chat_popup,
        );
        app.add_systems(Update, keyboard_popup);
        app.add_console_command::<Rechat, _>(debug_chat);
        app.add_event::<PrivateChatEntered>();
        app.add_plugins((FriendsPlugin, ChatHistoryPlugin));
    }
}

// panel component
#[derive(Component)]
pub struct ChatboxContainer;

// text line marker
#[derive(Component, Clone, Debug)]
pub struct DisplayChatMessage {
    pub timestamp: f64,
    pub sender: Option<Address>,
    pub message: String,
}

/// output widget
#[derive(Component)]
pub struct ChatBox {
    chat_log: RingBuffer<DisplayChatMessage>,
    pub active_tab: &'static str,
    active_chat_sink: Option<RingBufferReceiver<DisplayChatMessage>>,
    active_log_sink: Option<(Entity, RingBufferReceiver<SceneLogMessage>)>,
}

pub const BUTTON_SCALE: f32 = 6.0;

fn setup(mut commands: Commands, asset_server: Res<AssetServer>, ui_root: Res<SystemUiRoot>) {
    // profile button
    let button = commands
        .spawn((
            ImageBundle {
                image: asset_server.load("images/chat_button.png").into(),
                style: Style {
                    position_type: PositionType::Absolute,
                    top: Val::VMin(BUTTON_SCALE * 3.5),
                    right: Val::VMin(BUTTON_SCALE * 0.5),
                    width: Val::VMin(BUTTON_SCALE),
                    height: Val::VMin(BUTTON_SCALE),
                    ..Default::default()
                },
                focus_policy: bevy::ui::FocusPolicy::Block,
                ..Default::default()
            },
            Interaction::default(),
            On::<Click>::new(
                |mut commands: Commands, mut q: Query<&mut Style, With<ChatboxContainer>>| {
                    if let Ok(mut style) = q.get_single_mut() {
                        style.display = if style.display == Display::Flex {
                            commands
                                .fire_event(SystemAudio("sounds/ui/toggle_disable.wav".to_owned()));
                            Display::None
                        } else {
                            commands
                                .fire_event(SystemAudio("sounds/ui/toggle_enable.wav".to_owned()));
                            Display::Flex
                        };
                    }
                },
            ),
            On::<HoverEnter>::new(|mut tooltip: ResMut<ToolTips>| {
                tooltip.0.insert(
                    TooltipSource::Label("chat-button"),
                    vec![("Toggle Chat: Click or press Enter".to_owned(), true)],
                );
            }),
            On::<HoverExit>::new(|mut tooltip: ResMut<ToolTips>| {
                tooltip.0.remove(&TooltipSource::Label("chat-button"));
            }),
        ))
        .id();

    commands.entity(ui_root.0).push_children(&[button]);
}

fn keyboard_popup(
    mut commands: Commands,
    input_manager: InputManager,
    mut container: Query<&mut Style, With<ChatboxContainer>>,
    entry: Query<Entity, With<ChatInput>>,
) {
    if input_manager.just_down(SystemAction::Chat, InputPriority::None) {
        if let Ok(mut style) = container.get_single_mut() {
            if style.display == Display::None {
                commands.fire_event(SystemAudio("sounds/ui/toggle_enable.wav".to_owned()));
                style.display = Display::Flex;
            };
        }

        if let Ok(entry) = entry.get_single() {
            commands.entity(entry).insert(Focus);
        }
    }
}

fn debug_chat(
    mut input: ConsoleCommand<Rechat>,
    mut commands: Commands,
    root: Res<SystemUiRoot>,
    dui: Res<DuiRegistry>,
    existing: Query<(Entity, &DuiEntities), With<ChatboxContainer>>,
    mut tab: Query<&mut TabSelection, With<ChatTab>>,
) {
    if let Some(Ok(Rechat { arg })) = input.take() {
        match arg.as_str() {
            "reload" => {
                if let Ok((existing, _)) = existing.get_single() {
                    commands.entity(existing).despawn_recursive();
                }

                commands.fire_event(FriendshipEvent(None));
                setup_chat_popup(commands, root, dui);
            }
            "add" => {
                tab.single_mut()
                    .add(
                        &mut commands,
                        &dui,
                        Some(3),
                        DuiButton::new_enabled("new tab", || {}),
                        false,
                        None,
                    )
                    .unwrap();
            }
            _ => (),
        }
    }
}

#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/cdb")]
struct Rechat {
    arg: String,
}

#[derive(Component)]
pub struct ChatTab;

fn setup_chat_popup(mut commands: Commands, root: Res<SystemUiRoot>, dui: Res<DuiRegistry>) {
    dcl_assert!(root.0 != Entity::PLACEHOLDER);

    let chat_tab = |label: &'static str| -> DuiButton {
        DuiButton {
            text_size: Some(0.025 / 1.3),
            ..DuiButton::new_enabled(label, (move || Some(label)).pipe(select_chat_tab))
        }
    };

    let tab_labels = vec!["Nearby", "Scene Log"];
    let chat_tabs = tab_labels
        .clone()
        .into_iter()
        .map(chat_tab)
        .collect::<Vec<_>>();

    let tab_labels_changed = tab_labels.clone();
    let tab_changed =
        (move |selection: Query<&TabSelection, With<ChatTab>>| -> Option<&'static str> {
            Some(
                selection
                    .get_single()
                    .ok()
                    .and_then(|ts| ts.selected)
                    .and_then(|sel| tab_labels_changed.get(sel))
                    .unwrap_or(&""),
            )
        })
        .pipe(select_chat_tab);

    let close_ui = |mut commands: Commands, mut q: Query<&mut Style, With<ChatboxContainer>>| {
        let Ok(mut style) = q.get_single_mut() else {
            return;
        };
        style.display = Display::None;
        commands.fire_event(SystemAudio("sounds/ui/toggle_disable.wav".to_owned()));
    };

    let props = DuiProps::new()
        .with_prop("chat-tabs", chat_tabs)
        .with_prop("tab-changed", On::<DataChanged>::new(tab_changed))
        .with_prop("initial-tab", Some(0usize))
        .with_prop("close", On::<Click>::new(close_ui))
        .with_prop("friends", On::<Click>::new(toggle_friends));

    let components = commands
        .entity(root.0)
        .spawn_template(&dui, "chat", props)
        .unwrap();

    commands.entity(components.root).insert((
        ChatboxContainer,
        On::<Click>::new(
            |mut commands: Commands, q: Query<Entity, With<ChatInput>>| {
                commands.entity(q.single()).try_insert(Focus);
            },
        ),
    ));

    commands
        .entity(components.named("chat-entry"))
        .insert(ChatInput);

    commands
        .entity(components.named("chat-output-inner"))
        .insert(ChatBox {
            active_tab: "Nearby",
            chat_log: RingBuffer::new(100, 100),
            active_chat_sink: None,
            active_log_sink: None,
        });

    commands.entity(components.named("tabs")).insert(ChatTab);
}

fn toggle_friends(container: Query<&DuiEntities, With<ChatboxContainer>>, mut commands: Commands) {
    let components = container
        .get_single()
        .ok()
        .map(|ents| (ents.root, ents.named("friends-panel")));
    if let Some((container, friends)) = components {
        commands
            .entity(container)
            .modify_component(|style: &mut Style| {
                if style.width == Val::VMin(90.0) {
                    style.width = Val::VMin(45.0);
                    style.min_width = Val::VMin(30.0);
                } else {
                    style.width = Val::VMin(90.0);
                    style.min_width = Val::VMin(75.0);
                }
            });
        commands
            .entity(friends)
            .modify_component(|style: &mut Style| {
                style.display = if style.display == Display::Flex {
                    Display::None
                } else {
                    Display::Flex
                };
            });
    }
}

fn append_chat_messages(
    mut chats: EventReader<ChatEvent>,
    mut chatbox: Query<&mut ChatBox>,
    users: Query<&UserProfile>,
) {
    let Ok(mut chatbox) = chatbox.get_single_mut() else {
        return;
    };

    for ev in chats.read().filter(|ev| {
        !chat_marker_things::ALL
            .iter()
            .any(|marker| ev.message.starts_with(*marker))
    }) {
        let sender = if ev.sender == Entity::PLACEHOLDER {
            None
        } else {
            let Ok(profile) = users.get(ev.sender) else {
                warn!("can't get profile for chat sender {:?}", ev.sender);
                continue;
            };
            profile.content.eth_address.as_h160()
        };

        chatbox.chat_log.send(DisplayChatMessage {
            timestamp: ev.timestamp,
            sender,
            message: ev.message.to_owned(),
        });
    }
}

fn make_log(commands: &mut Commands, asset_server: &AssetServer, log: SceneLogMessage) -> Entity {
    let SceneLogMessage {
        timestamp,
        level,
        mut message,
    } = log;

    if message.len() > 1000 {
        message = format!(
            "{} ... [truncated]",
            message.chars().take(1000).collect::<String>()
        );
    }

    commands
        .spawn((
            DisplayChatMessage {
                timestamp,
                sender: None,
                message: message.clone(),
            },
            FontSize(0.0175),
            TextBundle {
                text: Text::from_sections([TextSection::new(
                    message,
                    TextStyle {
                        font: asset_server.load("fonts/NotoSans-Bold.ttf"),
                        font_size: 15.0,
                        color: match level {
                            SceneLogLevel::Log => Color::WHITE,
                            SceneLogLevel::SceneError => css::YELLOW.into(),
                            SceneLogLevel::SystemError => css::BISQUE.into(),
                        },
                    },
                )]),
                ..Default::default()
            },
        ))
        .id()
}

#[allow(clippy::too_many_arguments)]
fn display_chat(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut chatbox: Query<(Entity, &mut ChatBox, Option<&Children>)>,
    containing_scene: ContainingScene,
    player: Query<Entity, With<PrimaryUser>>,
    contexts: Query<&RendererSceneContext>,
    mut conversation: ConversationManager,
) {
    let Ok((entity, mut chatbox, maybe_children)) = chatbox.get_single_mut() else {
        return;
    };

    if let Some(children) = maybe_children {
        if children.len() > 255 {
            let mut iter = children.iter();
            for _ in 0..children.len() - 255 {
                commands.entity(*iter.next().unwrap()).despawn_recursive();
            }
        }
    }

    if chatbox.active_tab == "Nearby" {
        if chatbox.active_chat_sink.is_none() {
            let (.., receiver) = chatbox.chat_log.read();
            chatbox.active_chat_sink = Some(receiver);
        }

        let Some(rec) = chatbox.active_chat_sink.as_mut() else {
            panic!()
        };
        while let Ok(chat) = rec.try_recv() {
            conversation.add_message(
                entity,
                chat.sender.or(Some(Address::zero())),
                if chat.sender.is_none() {
                    Color::srgb(0.7, 0.7, 0.7)
                } else {
                    Color::srgb(0.9, 0.9, 0.9)
                },
                chat.message,
                false,
            );
        }

        return;
    }

    if chatbox.active_tab == "Scene Log" {
        let current_scene = player
            .get_single()
            .map(|player| containing_scene.get_parcel(player))
            .unwrap_or_default();

        if chatbox.active_log_sink.as_ref().map(|(id, _)| id) != current_scene.as_ref() {
            chatbox.active_log_sink = None;
            commands.entity(entity).despawn_descendants();

            if let Some(current_scene) = current_scene {
                if let Ok(context) = contexts.get(current_scene) {
                    let (missed, backlog, receiver) = context.logs.read();
                    chatbox.active_log_sink = Some((current_scene, receiver));
                    let mut msgs = Vec::default();

                    if missed > 0 {
                        msgs.push(make_log(
                            &mut commands,
                            &asset_server,
                            SceneLogMessage {
                                timestamp: 0.0,
                                level: SceneLogLevel::SystemError,
                                message: format!("(missed {missed} logs)"),
                            },
                        ));
                    }
                    for message in backlog.into_iter() {
                        msgs.push(make_log(&mut commands, &asset_server, message));
                    }
                    commands.entity(entity).replace_children(&msgs);
                }

                if let Some((_, ref mut rec)) = chatbox.active_log_sink.as_mut() {
                    while let Ok(log) = rec.try_recv() {
                        let msg = make_log(&mut commands, &asset_server, log);
                        commands.entity(entity).add_child(msg);
                    }
                }
            }
        } else if let Some((_, sink)) = chatbox.active_log_sink.as_mut() {
            let mut msgs = Vec::default();
            while let Ok(message) = sink.try_recv() {
                msgs.push(make_log(&mut commands, &asset_server, message));
            }
            commands.entity(entity).try_push_children(&msgs);
        }
    }
}

#[derive(Component)]
pub struct ChatInput;

#[derive(Event)]
pub struct PrivateChatEntered(pub String);

#[allow(clippy::too_many_arguments)]
fn emit_user_chat(
    mut commands: Commands,
    mut chats: EventWriter<ChatEvent>,
    mut private: EventWriter<PrivateChatEntered>,
    transports: Query<&Transport>,
    player: Query<Entity, With<PrimaryUser>>,
    time: Res<Time>,
    chat_input: Query<(Entity, &TextEntrySubmit), With<ChatInput>>,
    chat_output: Query<&ChatBox>,
    console_config: Res<ConsoleConfiguration>,
    mut command_entered: EventWriter<ConsoleCommandEntered>,
    mut console_lines: EventReader<PrintConsoleLine>,
    f: Query<Entity, With<Focus>>,
) {
    let Ok(player) = player.get_single() else {
        return;
    };
    let Ok(output) = chat_output.get_single() else {
        return;
    };

    if let Ok((ent, TextEntrySubmit(message))) = chat_input.get_single() {
        let mut cmds = commands.entity(ent);
        cmds.remove::<TextEntrySubmit>();

        if message.is_empty() {
            for e in f.iter() {
                commands.entity(e).remove::<Focus>();
            }
        } else {
            if output.active_tab.is_empty() {
                // private chat (what a hacky approach this is)
                private.send(PrivateChatEntered(message.clone()));
                return;
            }

            // cmds.insert(Focus);
            let sender = if message.starts_with('/') {
                Entity::PLACEHOLDER
            } else {
                player
            };

            chats.send(ChatEvent {
                timestamp: time.elapsed_seconds_f64(),
                sender,
                channel: output.active_tab.to_owned(),
                message: message.clone(),
            });

            if message.starts_with('/') {
                let mut args = Shlex::new(message).collect::<Vec<_>>();

                let command_name = args.remove(0);
                debug!("Command entered: `{command_name}`, with args: `{args:?}`");

                let command = console_config.commands.get(command_name.as_str());

                if command.is_some() {
                    command_entered.send(ConsoleCommandEntered { command_name, args });
                } else {
                    debug!(
                        "Command not recognized, recognized commands: `{:?}`",
                        console_config.commands.keys().collect::<Vec<_>>()
                    );
                }
            } else if output.active_tab == "Nearby" {
                commands.fire_event(SystemAudio(
                    "sounds/ui/widget_chat_message_private_send.wav".to_owned(),
                ));
                for transport in transports.iter() {
                    let _ = transport
                        .sender
                        .try_send(NetworkMessage::reliable(&rfc4::Packet {
                            message: Some(rfc4::packet::Message::Chat(rfc4::Chat {
                                message: message.clone(),
                                timestamp: time.elapsed_seconds_f64(),
                            })),
                            protocol_version: 100,
                        }));
                }
            }
        }
    }

    for PrintConsoleLine { line } in console_lines.read() {
        chats.send(ChatEvent {
            timestamp: time.elapsed_seconds_f64(),
            sender: Entity::PLACEHOLDER,
            channel: "Nearby".to_owned(),
            message: line.to_string(),
        });
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn select_chat_tab(
    In(tab): In<Option<&'static str>>,
    mut commands: Commands,
    mut chatbox: Query<(Entity, &mut ChatBox)>,
    mut text_entry: Query<&mut TextEntry, With<ChatInput>>,
    mut conversation: ConversationManager,
) {
    let Some(tab) = tab else {
        return;
    };

    let Ok((entity, mut chatbox)) = chatbox.get_single_mut() else {
        return;
    };

    let clicked_current = chatbox.active_tab == tab;

    if !clicked_current {
        commands.entity(entity).despawn_descendants();
        chatbox.active_log_sink = None;
        chatbox.active_chat_sink = None;
        if tab == "Nearby" {
            conversation.clear(entity);
            let (_, backlog, receiver) = chatbox.chat_log.read();
            chatbox.active_chat_sink = Some(receiver);
            for message in backlog.into_iter() {
                conversation.add_message(
                    entity,
                    message.sender.or(Some(Address::zero())),
                    if message.sender.is_none() {
                        Color::srgb(0.7, 0.7, 0.7)
                    } else {
                        Color::srgb(0.9, 0.9, 0.9)
                    },
                    message.message,
                    false,
                );
            }
            text_entry.single_mut().enabled = true;
        } else if tab == "Scene Log" {
            text_entry.single_mut().enabled = false;
        }

        debug!("tab set to {}", tab);
        chatbox.active_tab = tab;
    }
}

fn pipe_chats_to_scene(
    mut chat_events: EventReader<ChatEvent>,
    mut requests: EventReader<SystemApi>,
    mut senders: Local<Vec<tokio::sync::mpsc::UnboundedSender<ChatMessage>>>,
    players: Query<&ForeignPlayer>,
    primary_player: Res<PrimaryPlayerRes>,
    wallet: Res<Wallet>,
) {
    senders.extend(requests.read().filter_map(|ev| {
        if let SystemApi::GetChatStream(sender) = ev {
            println!("got sender");
            Some(sender.clone())
        } else {
            None
        }
    }));
    senders.retain(|s| !s.is_closed());

    if senders.is_empty() {
        println!("no senders");
    }

    for chat_event in chat_events.read().filter(|ce| {
        ce.sender != Entity::PLACEHOLDER && !ce.message.starts_with(chat_marker_things::EMOTE)
    }) {
        let Some(player_address) = players
            .get(chat_event.sender)
            .ok()
            .map(|fp| fp.address)
            .or_else(|| {
                if chat_event.sender == primary_player.0 {
                    wallet.address()
                } else {
                    None
                }
            })
        else {
            warn!("no player for {chat_event:?}");
            continue;
        };

        for sender in senders.iter() {
            let _ = sender.send(ChatMessage {
                sender_address: format!("{:#x}", player_address),
                message: chat_event.message.clone(),
                channel: chat_event.channel.clone(),
            });
        }
    }
}

fn pipe_chats_from_scene(
    mut sender: EventWriter<ChatEvent>,
    primary_player: Res<PrimaryPlayerRes>,
    mut chats: EventReader<SystemApi>,
    time: Res<Time>,
) {
    for (message, channel) in chats.read().filter_map(|ev| {
        if let SystemApi::SendChat(message, channel) = ev {
            Some((message.clone(), channel.clone()))
        } else {
            None
        }
    }) {
        sender.send(ChatEvent {
            timestamp: time.elapsed_seconds_f64(),
            sender: primary_player.0,
            channel,
            message,
        });
    }
}
