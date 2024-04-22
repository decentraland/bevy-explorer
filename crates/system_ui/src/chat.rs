use bevy::{core::FrameCount, prelude::*};

use bevy_console::{ConsoleCommandEntered, ConsoleConfiguration, PrintConsoleLine};
use bevy_dui::{DuiCommandsExt, DuiProps, DuiRegistry};
use common::{
    dcl_assert,
    structs::{PrimaryUser, ToolTips},
    util::{RingBuffer, RingBufferReceiver, TryPushChildrenEx},
};
use comms::{
    chat_marker_things, global_crdt::ChatEvent, profile::UserProfile, NetworkMessage, Transport,
};
use copypasta::{ClipboardContext, ClipboardProvider};
use dcl::{SceneLogLevel, SceneLogMessage};
use dcl_component::proto_components::kernel::comms::rfc4;
use input_manager::should_accept_key;
use scene_runner::{renderer_context::RendererSceneContext, ContainingScene, Toaster};
use shlex::Shlex;
use ui_core::{
    button::{DuiButton, TabSelection},
    focus::Focus,
    text_size::FontSize,
    textentry::TextEntry,
    ui_actions::{Click, DataChanged, HoverEnter, HoverExit, On, UiCaller},
};

use super::SystemUiRoot;

pub struct ChatPanelPlugin;

impl Plugin for ChatPanelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, display_chat);
        app.add_systems(Update, append_chat_messages);
        app.add_systems(Update, emit_user_chat);
        app.add_systems(Startup, setup);
        app.add_systems(OnEnter::<ui_core::State>(ui_core::State::Ready), chat_popup);
        app.add_systems(
            Update,
            (keyboard_popup.pipe(select_chat_tab)).run_if(should_accept_key),
        );
    }
}

// panel component
#[derive(Component)]
pub struct ChatboxContainer;

// text line marker
#[derive(Component, Clone, Debug)]
pub struct DisplayChatMessage {
    pub timestamp: f64,
    pub sender: Option<String>,
    pub message: String,
}

/// output widget
#[derive(Component)]
pub struct ChatBox {
    chat_log: RingBuffer<DisplayChatMessage>,
    active_tab: &'static str,
    active_chat_sink: Option<RingBufferReceiver<DisplayChatMessage>>,
    active_log_sink: Option<(Entity, RingBufferReceiver<SceneLogMessage>)>,
}

pub const BUTTON_SCALE: f32 = 6.0;

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    // profile button
    commands.spawn((
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
        On::<Click>::new(|mut q: Query<&mut Style, With<ChatboxContainer>>| {
            if let Ok(mut style) = q.get_single_mut() {
                style.display = if style.display == Display::Flex {
                    Display::None
                } else {
                    Display::Flex
                };
            }
        }),
        On::<HoverEnter>::new(|mut tooltip: ResMut<ToolTips>| {
            tooltip.0.insert(
                "chat-button",
                vec![("Toggle Chat: Click or press Enter".to_owned(), true)],
            );
        }),
        On::<HoverExit>::new(|mut tooltip: ResMut<ToolTips>| {
            tooltip.0.remove("chat-button");
        }),
    ));
}

fn keyboard_popup(
    mut commands: Commands,
    input: Res<ButtonInput<KeyCode>>,
    mut container: Query<&mut Style, With<ChatboxContainer>>,
    entry: Query<Entity, With<ChatInput>>,
) -> &'static str {
    let mut res = "";
    if input.just_pressed(KeyCode::Enter) || input.just_pressed(KeyCode::NumpadEnter) {
        if let Ok(mut style) = container.get_single_mut() {
            if style.display == Display::None {
                style.display = Display::Flex;
                res = "Nearby";
            };
        }

        if let Ok(entry) = entry.get_single() {
            commands.entity(entry).insert(Focus);
        }
    }

    res
}

fn chat_popup(mut commands: Commands, root: Res<SystemUiRoot>, dui: Res<DuiRegistry>) {
    dcl_assert!(root.0 != Entity::PLACEHOLDER);

    let chat_tab = |label: &'static str| -> DuiButton {
        DuiButton::new_enabled(label, (move || label).pipe(select_chat_tab))
    };

    let tab_labels = vec!["Nearby", "Scene Log", "System Log"];
    let chat_tabs = tab_labels
        .clone()
        .into_iter()
        .map(chat_tab)
        .collect::<Vec<_>>();

    let tab_labels_changed = tab_labels.clone();
    let tab_changed =
        (move |caller: Res<UiCaller>, selection: Query<&TabSelection>| -> &'static str {
            selection
                .get(caller.0)
                .ok()
                .and_then(|ts| ts.selected)
                .and_then(|sel| tab_labels_changed.get(sel))
                .unwrap_or(&"")
        })
        .pipe(select_chat_tab);

    let close_ui = |mut q: Query<&mut Style, With<ChatboxContainer>>| {
        let Ok(mut style) = q.get_single_mut() else {
            return;
        };
        style.display = Display::None;
    };

    let props = DuiProps::new()
        .with_prop("chat-tabs", chat_tabs)
        .with_prop("tab-changed", On::<DataChanged>::new(tab_changed))
        .with_prop("initial-tab", Some(0usize))
        .with_prop("close", On::<Click>::new(close_ui))
        .with_prop("copy", On::<Click>::new(copy_chat));

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

    // commands
    //     .entity(components.named("chat-output"))
    //     .insert(BackgroundColor(Color::rgba(0.0, 0.0, 0.25, 0.2)));

    commands
        .entity(components.named("chat-output-inner"))
        .insert(ChatBox {
            active_tab: "Nearby",
            chat_log: RingBuffer::new(100, 100),
            active_chat_sink: None,
            active_log_sink: None,
        });
}

fn copy_chat(
    chatbox: Query<&Children, With<ChatBox>>,
    msgs: Query<&DisplayChatMessage>,
    mut toaster: Toaster,
    frame: Res<FrameCount>,
) {
    let mut copy = String::default();
    let Ok(children) = chatbox.get_single() else {
        return;
    };
    for ent in children.iter() {
        if let Ok(msg) = msgs.get(*ent) {
            copy.push_str(
                format!(
                    "[{}] {}\n",
                    msg.sender.as_deref().unwrap_or("log"),
                    msg.message
                )
                .as_str(),
            );
        }
    }

    let label = format!("chatcopy {}", frame.0);

    if let Ok(mut ctx) = ClipboardContext::new() {
        if ctx.set_contents(copy).is_ok() {
            toaster.add_toast(&label, "history copied to clipboard");
            return;
        }
    }

    toaster.add_toast("chat copy", "failed to set clipboard ...");
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
            Some(profile.content.name.to_owned())
        };

        chatbox.chat_log.send(DisplayChatMessage {
            timestamp: ev.timestamp,
            sender,
            message: ev.message.to_owned(),
        });
    }
}

fn make_chat(
    commands: &mut Commands,
    asset_server: &AssetServer,
    msg: DisplayChatMessage,
) -> Entity {
    commands
        .spawn((
            FontSize(0.0175),
            TextBundle {
                text: Text::from_sections([
                    TextSection::new(
                        format!(
                            "{}: ",
                            msg.sender.clone().unwrap_or_else(|| "[SYSTEM]".to_owned())
                        ),
                        TextStyle {
                            font: asset_server.load("fonts/NotoSans-Bold.ttf"),
                            font_size: 15.0,
                            color: if msg.sender.is_some() {
                                Color::YELLOW
                            } else {
                                Color::GRAY
                            },
                        },
                    ),
                    TextSection::new(
                        msg.message.clone(),
                        TextStyle {
                            font: asset_server.load("fonts/NotoSans-Bold.ttf"),
                            font_size: 15.0,
                            color: if msg.sender.is_some() {
                                Color::WHITE
                            } else {
                                Color::GRAY
                            },
                        },
                    ),
                ]),
                ..Default::default()
            },
            msg,
        ))
        .id()
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
                            SceneLogLevel::SceneError => Color::YELLOW,
                            SceneLogLevel::SystemError => Color::BISQUE,
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
            let msg = make_chat(&mut commands, &asset_server, chat);
            commands.entity(entity).add_child(msg);
        }
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

#[allow(clippy::too_many_arguments)]
fn emit_user_chat(
    mut chats: EventWriter<ChatEvent>,
    transports: Query<&Transport>,
    player: Query<Entity, With<PrimaryUser>>,
    time: Res<Time>,
    mut chat_input: Query<&mut TextEntry, With<ChatInput>>,
    chat_output: Query<&ChatBox>,
    console_config: Res<ConsoleConfiguration>,
    mut command_entered: EventWriter<ConsoleCommandEntered>,
    mut console_lines: EventReader<PrintConsoleLine>,
) {
    let Ok(player) = player.get_single() else {
        return;
    };
    let Ok(mut textbox) = chat_input.get_single_mut() else {
        return;
    };
    let Ok(output) = chat_output.get_single() else {
        return;
    };

    for message in textbox.messages.drain(..) {
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
            let mut args = Shlex::new(&message).collect::<Vec<_>>();

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
            for transport in transports.iter() {
                let _ = transport
                    .sender
                    .try_send(NetworkMessage::reliable(&rfc4::Packet {
                        message: Some(rfc4::packet::Message::Chat(rfc4::Chat {
                            message: message.clone(),
                            timestamp: time.elapsed_seconds_f64(),
                        })),
                    }));
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
fn select_chat_tab(
    In(tab): In<&'static str>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut chatbox: Query<(Entity, &mut ChatBox)>,
    mut text_entry: Query<&mut TextEntry, With<ChatInput>>,
    time: Res<Time>,
) {
    if tab.is_empty() {
        return;
    }
    let (entity, mut chatbox) = chatbox.single_mut();

    let clicked_current = chatbox.active_tab == tab;

    if !clicked_current {
        commands.entity(entity).despawn_descendants();
        chatbox.active_log_sink = None;
        chatbox.active_chat_sink = None;
        if tab == "Nearby" {
            let mut msgs = Vec::new();
            let (missed, backlog, receiver) = chatbox.chat_log.read();
            chatbox.active_chat_sink = Some(receiver);
            if missed > 0 {
                msgs.push(make_chat(
                    &mut commands,
                    &asset_server,
                    DisplayChatMessage {
                        timestamp: time.elapsed_seconds_f64(),
                        sender: None,
                        message: format!("(missed {missed} prior messages)"),
                    },
                ));
            }
            for message in backlog.into_iter() {
                msgs.push(make_chat(&mut commands, &asset_server, message));
            }
            commands.entity(entity).replace_children(&msgs);
            text_entry.single_mut().enabled = true;
        } else {
            text_entry.single_mut().enabled = false;
        }

        debug!("tab set to {}", tab);
        chatbox.active_tab = tab;
    }
}
