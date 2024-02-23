use bevy::{
    core::FrameCount,
    prelude::*,
    ui::{self, FocusPolicy},
};

use common::{
    dcl_assert,
    sets::SetupSets,
    structs::PrimaryUser,
    util::{RingBuffer, RingBufferReceiver, TryPushChildrenEx},
};
use comms::{
    chat_marker_things, global_crdt::ChatEvent, profile::UserProfile, NetworkMessage, Transport,
};
use copypasta::{ClipboardContext, ClipboardProvider};
use dcl::{SceneLogLevel, SceneLogMessage};
use dcl_component::proto_components::kernel::comms::rfc4;
use scene_runner::{renderer_context::RendererSceneContext, ContainingScene, Toaster};
use ui_core::{
    focus::Focus,
    interact_style::{Active, InteractStyle, InteractStyles},
    scrollable::{ScrollDirection, Scrollable, SpawnScrollable, StartPosition},
    textentry::TextEntry,
    ui_actions::{Click, Defocus, HoverEnter, HoverExit, On},
};

use super::SystemUiRoot;

pub struct ChatPanelPlugin;

impl Plugin for ChatPanelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, display_chat);
        app.add_systems(Update, append_chat_messages);
        app.add_systems(Update, emit_user_chat);
        app.add_systems(
            Startup,
            setup
                .in_set(SetupSets::Main)
                .after(SetupSets::Init)
                .after(crate::sysinfo::setup),
        );
    }
}

// panel component
#[derive(Component)]
pub struct ChatboxContainer;

// text line marker
#[derive(Component)]
pub struct DisplayChatMessage {
    pub timestamp: f64,
    pub sender: Option<String>,
    pub message: String,
}

/// output widget
#[derive(Component)]
pub struct ChatBox {
    chat_log: RingBuffer<(f64, String, String)>,
    active_tab: &'static str,
    active_chat_sink: Option<RingBufferReceiver<(f64, String, String)>>,
    active_log_sink: Option<(Entity, RingBufferReceiver<SceneLogMessage>)>,
}

// container for tab buttons
#[derive(Component)]
pub struct ChatTabs;

#[derive(Component)]
pub struct ChatButton(&'static str);

#[derive(Component)]
pub struct ChatOutput;

// hide when chatbox is hidden
#[derive(Component)]
pub struct ChatToggle;

fn setup(mut commands: Commands, asset_server: Res<AssetServer>, root: Res<SystemUiRoot>) {
    let tabstyle = TextStyle {
        font: asset_server.load("fonts/FiraSans-Bold.ttf"),
        font_size: 20.0,
        color: Color::rgb(0.1, 0.1, 0.1),
    };

    dcl_assert!(root.0 != Entity::PLACEHOLDER);

    // chat box
    commands.entity(root.0).with_children(|commands| {
        commands
            .spawn((
                NodeBundle {
                    style: ui::Style {
                        // TODO: use a percent size here
                        width: Val::Percent(30.0),
                        height: Val::Percent(20.0),
                        min_width: Val::Px(300.0),
                        min_height: Val::Px(200.0),
                        flex_direction: FlexDirection::Column,
                        justify_content: JustifyContent::FlexEnd,
                        ..Default::default()
                    },
                    focus_policy: FocusPolicy::Block,
                    ..Default::default()
                },
                ChatboxContainer,
                Interaction::None,
                On::<HoverEnter>::new(update_chatbox_focus),
                On::<HoverExit>::new(update_chatbox_focus),
                On::<Click>::new(
                    |mut commands: Commands, q: Query<Entity, With<ChatInput>>| {
                        commands.entity(q.single()).try_insert(Focus);
                    },
                ),
            ))
            .with_children(|commands| {
                // buttons
                commands
                    .spawn((
                        NodeBundle {
                            style: ui::Style {
                                flex_direction: FlexDirection::Row,
                                justify_content: JustifyContent::FlexStart,
                                align_items: AlignItems::FlexEnd,
                                flex_wrap: FlexWrap::Wrap,
                                width: Val::Percent(100.0),
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                        ChatTabs,
                    ))
                    .with_children(|commands| {
                        let make_button = |commands: &mut ChildBuilder,
                                           label: &'static str,
                                           active: bool| {
                            commands
                                .spawn((
                                    ButtonBundle {
                                        style: Style {
                                            border: UiRect::all(Val::Px(5.0)),
                                            margin: UiRect {
                                                bottom: Val::Px(1.0),
                                                ..UiRect::all(Val::Px(3.0))
                                            },
                                            ..Default::default()
                                        },
                                        focus_policy: FocusPolicy::Pass,
                                        ..Default::default()
                                    },
                                    InteractStyles {
                                        active: Some(InteractStyle {
                                            background: Some(Color::rgba(1.0, 1.0, 1.0, 1.0)),
                                            ..Default::default()
                                        }),
                                        hover: Some(InteractStyle {
                                            background: Some(Color::rgba(0.7, 0.7, 0.7, 1.0)),
                                            ..Default::default()
                                        }),
                                        inactive: Some(InteractStyle {
                                            background: Some(Color::rgba(0.4, 0.4, 0.4, 1.0)),
                                            ..Default::default()
                                        }),
                                        ..Default::default()
                                    },
                                    On::<Click>::new((move || label).pipe(select_chat_tab)),
                                    ChatButton(label),
                                    Active(active),
                                ))
                                .with_children(|commands| {
                                    commands
                                        .spawn(TextBundle::from_section(label, tabstyle.clone()));
                                });
                        };

                        // TODO take chat titles from the transport / link to transport
                        make_button(commands, "Nearby", true);
                        make_button(commands, "Scene Log", false);
                        make_button(commands, "System Log", false);
                    });

                // chat display
                commands.spawn_scrollable(
                    (
                        NodeBundle {
                            style: ui::Style {
                                width: Val::Percent(100.0),
                                height: Val::Percent(30.0),
                                max_width: Val::Percent(100.0),
                                max_height: Val::Percent(80.0),
                                min_width: Val::Percent(100.0),
                                min_height: Val::Percent(30.0),
                                flex_grow: 1.0,
                                overflow: Overflow::clip(),
                                ..Default::default()
                            },
                            background_color: BackgroundColor(Color::rgba(0.0, 0.0, 0.25, 0.2)),
                            ..Default::default()
                        },
                        Interaction::default(),
                        ChatToggle,
                        ChatOutput,
                    ),
                    Scrollable::new()
                        .with_wheel(true)
                        .with_drag(true)
                        .with_direction(ScrollDirection::Vertical(StartPosition::End)),
                    |commands| {
                        commands.spawn((
                            NodeBundle {
                                style: ui::Style {
                                    flex_direction: FlexDirection::Column,
                                    justify_content: JustifyContent::FlexEnd,
                                    width: Val::Percent(100.0),
                                    height: Val::Auto,
                                    ..Default::default()
                                },
                                ..Default::default()
                            },
                            ChatBox {
                                active_tab: "Nearby",
                                chat_log: RingBuffer::new(100, 100),
                                active_chat_sink: None,
                                active_log_sink: None,
                            },
                            Interaction::default(),
                        ));
                    },
                );

                // chat entry line
                commands
                    .spawn(NodeBundle {
                        style: ui::Style {
                            border: UiRect::all(Val::Px(5.0)),
                            flex_direction: FlexDirection::Row,
                            justify_content: JustifyContent::FlexEnd,
                            width: Val::Percent(100.0),
                            height: Val::Px(20.0),
                            ..Default::default()
                        },
                        background_color: BackgroundColor(Color::rgba(0.0, 0.0, 0.2, 0.8)),
                        ..Default::default()
                    })
                    .with_children(|commands| {
                        commands.spawn((
                            NodeBundle {
                                style: ui::Style {
                                    // border: UiRect::all(Val::Px(5.0)),
                                    flex_direction: FlexDirection::Column,
                                    justify_content: JustifyContent::FlexEnd,
                                    align_content: AlignContent::Center,
                                    width: Val::Percent(100.0),
                                    height: Val::Px(20.0),
                                    ..Default::default()
                                },
                                background_color: BackgroundColor(Color::rgba(0.0, 0.0, 0.2, 0.8)),
                                ..Default::default()
                            },
                            TextEntry {
                                enabled: true,
                                accept_line: true,
                                ..Default::default()
                            },
                            ChatInput,
                            ChatToggle,
                            Interaction::default(),
                            On::<Defocus>::new(update_chatbox_focus),
                        ));

                        commands.spawn((
                            ImageBundle {
                                style: ui::Style {
                                    top: Val::Px(-5.0),
                                    width: Val::Px(20.0),
                                    height: Val::Px(20.0),
                                    ..Default::default()
                                },
                                image: asset_server.load("images/copy.png").into(),
                                ..Default::default()
                            },
                            Interaction::default(),
                            On::<Click>::new(
                                |chatbox: Query<&Children, With<ChatBox>>,
                                 msgs: Query<&DisplayChatMessage>,
                                 mut toaster: Toaster,
                                 frame: Res<FrameCount>| {
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
                                            toaster
                                                .add_toast(&label, "history copied to clipboard");
                                            return;
                                        }
                                    }

                                    toaster.add_toast("chat copy", "failed to set clipboard ...");
                                },
                            ),
                        ));
                    });
            });
    });
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
        let Ok(profile) = users.get(ev.sender) else {
            warn!("can't get profile for chat sender {:?}", ev.sender);
            continue;
        };

        chatbox.chat_log.send((
            ev.timestamp,
            profile.content.name.to_owned(),
            ev.message.to_owned(),
        ));
    }
}

fn make_chat(
    commands: &mut Commands,
    asset_server: &AssetServer,
    (timestamp, sender, message): (f64, String, String),
) -> Entity {
    commands
        .spawn((
            DisplayChatMessage {
                timestamp,
                sender: Some(sender.clone()),
                message: message.clone(),
            },
            TextBundle {
                text: Text::from_sections([
                    TextSection::new(
                        format!("{}: ", sender),
                        TextStyle {
                            font: asset_server.load("fonts/FiraSans-Bold.ttf"),
                            font_size: 15.0,
                            color: Color::YELLOW,
                        },
                    ),
                    TextSection::new(
                        message,
                        TextStyle {
                            font: asset_server.load("fonts/FiraSans-Bold.ttf"),
                            font_size: 15.0,
                            color: Color::WHITE,
                        },
                    ),
                ]),
                ..Default::default()
            },
        ))
        .id()
}

fn make_log(commands: &mut Commands, asset_server: &AssetServer, log: SceneLogMessage) -> Entity {
    let SceneLogMessage {
        timestamp,
        level,
        message,
    } = log;
    commands
        .spawn((
            DisplayChatMessage {
                timestamp,
                sender: None,
                message: message.clone(),
            },
            TextBundle {
                text: Text::from_sections([TextSection::new(
                    message,
                    TextStyle {
                        font: asset_server.load("fonts/FiraSans-Bold.ttf"),
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
    mut chatbox: Query<(Entity, &mut ChatBox)>,
    containing_scene: ContainingScene,
    player: Query<Entity, With<PrimaryUser>>,
    contexts: Query<&RendererSceneContext>,
) {
    let (entity, mut chatbox) = chatbox.single_mut();

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

fn emit_user_chat(
    mut chats: EventWriter<ChatEvent>,
    transports: Query<&Transport>,
    player: Query<Entity, With<PrimaryUser>>,
    time: Res<Time>,
    mut chat_input: Query<&mut TextEntry, With<ChatInput>>,
    chat_output: Query<&ChatBox>,
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
        if output.active_tab == "Nearby" {
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

            chats.send(ChatEvent {
                timestamp: time.elapsed_seconds_f64(),
                sender: player,
                channel: output.active_tab.to_owned(),
                message,
            });
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn select_chat_tab(
    In(tab): In<&'static str>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut chatbox: Query<(Entity, &mut ChatBox)>,
    mut toggles: Query<&mut Style, With<ChatToggle>>,
    mut buttons: Query<(&ChatButton, &mut Active)>,
    mut text_entry: Query<&mut TextEntry, With<ChatInput>>,
    time: Res<Time>,
) {
    let (entity, mut chatbox) = chatbox.single_mut();

    let clicked_current = chatbox.active_tab == tab;
    let visible = matches!(toggles.iter().next().unwrap().display, Display::Flex);

    let new_vis = if clicked_current && visible {
        Display::None
    } else {
        Display::Flex
    };

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
                    (
                        time.elapsed_seconds_f64(),
                        "System".to_owned(),
                        format!("(missed {missed} prior messages)"),
                    ),
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
        chatbox.active_tab = tab;
    }

    for (button, mut active) in buttons.iter_mut() {
        if button.0 == tab {
            active.0 = !(clicked_current && visible);
        } else {
            active.0 = false;
        }
    }

    for mut style in toggles.iter_mut() {
        style.display = new_vis;
    }
}

fn update_chatbox_focus(
    interaction: Query<&Interaction, With<ChatboxContainer>>,
    mut chat: Query<&mut BackgroundColor, With<ChatOutput>>,
    focused_input: Query<(), (With<ChatInput>, With<Focus>)>,
) {
    let mut bg = chat.single_mut();
    let interaction = interaction.single();

    // keep focus if either input has focus, or we are hovering
    if focused_input.get_single().is_ok() || !matches!(interaction, Interaction::None) {
        bg.0 = Color::rgba(0.0, 0.0, 0.25, 0.8);
    } else {
        bg.0 = Color::rgba(0.0, 0.0, 0.25, 0.2);
    }
}
