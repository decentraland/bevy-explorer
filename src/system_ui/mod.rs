pub mod click_actions;
pub mod focus;
pub mod interact_style;
pub mod textbox;

use bevy::ui::{self, FocusPolicy};
use bevy::{
    diagnostic::{Diagnostics, FrameTimeDiagnosticsPlugin},
    prelude::*,
};
use bevy_egui::EguiPlugin;

use crate::{
    comms::{global_crdt::ChatEvent, profile::UserProfile, NetworkMessage, Transport},
    dcl::{SceneLogLevel, SceneLogMessage},
    dcl_component::proto_components::kernel::comms::rfc4,
    scene_runner::{renderer_context::RendererSceneContext, ContainingScene, PrimaryUser},
    util::{RingBuffer, RingBufferReceiver},
    AppConfig,
};

use self::{
    click_actions::{UiActionPlugin, UiActions},
    focus::{Focus, FocusPlugin},
    interact_style::{Active, InteractStyle, InteractStylePlugin, InteractStyles},
    textbox::{update_textboxes, TextBox},
};

pub struct SystemUiPlugin;

impl Plugin for SystemUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_startup_system(setup);
        app.add_system(display_chat);
        app.add_system(append_chat_messages);
        app.add_system(update_fps);
        app.add_system(emit_user_chat);
        app.add_plugin(EguiPlugin);
        app.add_plugin(UiActionPlugin);
        app.add_plugin(FocusPlugin);
        app.add_plugin(InteractStylePlugin);
        app.add_system(update_textboxes);
    }
}

#[derive(Component)]
pub struct ChatBox {
    chat_log: RingBuffer<(f64, String, String)>,
    active_tab: &'static str,
    active_chat_sink: Option<RingBufferReceiver<(f64, String, String)>>,
    active_log_sink: Option<(Entity, RingBufferReceiver<SceneLogMessage>)>,
}

#[derive(Component)]
pub struct ChatTabs;

#[derive(Component)]
pub struct ChatboxContainer;

#[allow(clippy::type_complexity)]
fn setup(
    mut commands: Commands,
    mut actions: ResMut<UiActions>,
    asset_server: Res<AssetServer>,
    config: Res<AppConfig>,
) {
    let tabstyle = TextStyle {
        font: asset_server.load("fonts/FiraSans-Bold.ttf"),
        font_size: 20.0,
        color: Color::rgb(0.1, 0.1, 0.1),
    };

    commands
        .spawn((NodeBundle {
            style: ui::Style {
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::SpaceBetween,
                size: Size {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                },
                ..Default::default()
            },
            ..Default::default()
        },))
        .with_children(|commands| {
            // fps counter
            if config.graphics.log_fps {
                commands
                    .spawn(NodeBundle {
                        style: Style {
                            position_type: PositionType::Relative,
                            size: Size::all(Val::Percent(100.)),
                            justify_content: JustifyContent::SpaceBetween,
                            ..default()
                        },
                        ..default()
                    })
                    .with_children(|parent| {
                        // left vertical fill (border)
                        parent
                            .spawn(NodeBundle {
                                style: Style {
                                    size: Size::new(Val::Px(80.), Val::Px(30.)),
                                    border: UiRect::all(Val::Px(2.)),
                                    ..default()
                                },
                                background_color: Color::rgb(0.15, 0.15, 0.15).into(),
                                ..default()
                            })
                            .with_children(|parent| {
                                // text
                                parent.spawn((
                                    TextBundle::from_section(
                                        "Text Example",
                                        TextStyle {
                                            font: asset_server.load("fonts/FiraSans-Bold.ttf"),
                                            font_size: 20.0,
                                            color: Color::GREEN,
                                        },
                                    )
                                    .with_style(Style {
                                        margin: UiRect::all(Val::Px(5.)),
                                        ..default()
                                    }),
                                    FpsLabel,
                                ));
                            });
                    });
            }

            // chat box
            commands
                .spawn((
                    NodeBundle {
                        style: ui::Style {
                            size: Size {
                                // TODO: use a percent size here
                                // unfortunately text wrapping fails with percent sizes in bevy 0.10
                                width: Val::Px(640.0),
                                height: Val::Percent(50.0),
                            },
                            min_size: Size {
                                width: Val::Px(640.0),
                                height: Val::Px(120.0),
                            },
                            max_size: Size {
                                width: Val::Px(640.0),
                                height: Val::Percent(50.0),
                            },
                            flex_direction: FlexDirection::Column,
                            justify_content: JustifyContent::FlexEnd,
                            align_items: AlignItems::Stretch,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    ChatboxContainer,
                    Interaction::None,
                    actions.on_hover_enter(update_chatbox_focus),
                    actions.on_hover_exit(update_chatbox_focus),
                    actions.on_click(
                        |mut commands: Commands, q: Query<Entity, With<ChatInput>>| {
                            commands.entity(q.single()).insert(Focus);
                            // commands.entity(q.single()).remove::<Focus>().insert(Focus);
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
                                    flex_wrap: FlexWrap::Wrap,
                                    size: Size::width(Val::Percent(100.0)),
                                    ..Default::default()
                                },
                                ..Default::default()
                            },
                            ChatTabs,
                        ))
                        .with_children(|commands| {
                            let mut make_button =
                                |commands: &mut ChildBuilder, label: &'static str, active: bool| {
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
                                                active: InteractStyle {
                                                    background: Some(Color::rgba(
                                                        1.0, 1.0, 1.0, 1.0,
                                                    )),
                                                },
                                                hover: InteractStyle {
                                                    background: Some(Color::rgba(
                                                        0.7, 0.7, 0.7, 1.0,
                                                    )),
                                                },
                                                inactive: InteractStyle {
                                                    background: Some(Color::rgba(
                                                        0.4, 0.4, 0.4, 1.0,
                                                    )),
                                                },
                                            },
                                            actions.on_click((move || label).pipe(select_chat_tab)),
                                            ChatButton(label),
                                            Active(active),
                                        ))
                                        .with_children(|commands| {
                                            commands.spawn(TextBundle::from_section(
                                                label,
                                                tabstyle.clone(),
                                            ));
                                        });
                                };

                            make_button(commands, "Nearby", true);
                            make_button(commands, "Scene Log", false);
                            make_button(commands, "System Log", false);
                        });

                    // chat display
                    commands
                        .spawn((NodeBundle {
                            style: ui::Style {
                                size: Size {
                                    width: Val::Percent(100.0),
                                    height: Val::Auto,
                                },
                                flex_grow: 1.0,
                                // overflow: Overflow::Hidden,
                                ..Default::default()
                            },
                            ..Default::default()
                        },))
                        .with_children(|commands| {
                            commands.spawn((
                                NodeBundle {
                                    style: ui::Style {
                                        flex_direction: FlexDirection::Column,
                                        justify_content: JustifyContent::FlexEnd,
                                        size: Size {
                                            width: Val::Percent(100.0),
                                            height: Val::Auto,
                                        },
                                        max_size: Size {
                                            width: Val::Percent(100.0),
                                            height: Val::Auto,
                                        },
                                        ..Default::default()
                                    },
                                    background_color: BackgroundColor(Color::rgba(
                                        0.0, 0.0, 0.5, 0.2,
                                    )),
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
                        });

                    // chat entry line
                    commands.spawn((
                        NodeBundle {
                            style: ui::Style {
                                border: UiRect::all(Val::Px(5.0)),
                                flex_direction: FlexDirection::Column,
                                justify_content: JustifyContent::FlexEnd,
                                size: Size {
                                    width: Val::Percent(100.0),
                                    height: Val::Px(20.0),
                                },
                                ..Default::default()
                            },
                            background_color: BackgroundColor(Color::rgba(0.0, 0.0, 0.2, 0.8)),
                            ..Default::default()
                        },
                        TextBox {
                            enabled: true,
                            ..Default::default()
                        },
                        ChatInput,
                        Interaction::default(),
                        actions.on_defocus(update_chatbox_focus),
                    ));
                });
        });
}

#[derive(Component)]
pub struct DisplayChatMessage {
    pub timestamp: f64,
}

fn append_chat_messages(
    mut chats: EventReader<ChatEvent>,
    mut chatbox: Query<&mut ChatBox>,
    users: Query<&UserProfile>,
) {
    let Ok(mut chatbox) = chatbox.get_single_mut() else {
        return;
    };

    for ev in chats.iter() {
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
            DisplayChatMessage { timestamp },
            TextBundle {
                style: Style {
                    flex_wrap: FlexWrap::Wrap,
                    max_size: Size::width(Val::Px(640.0)),
                    ..Default::default()
                },
                text: Text::from_sections(
                    [
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
                    ]
                    .into_iter(),
                ),
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
            DisplayChatMessage { timestamp },
            TextBundle {
                style: Style {
                    max_size: Size::width(Val::Px(640.0)),
                    ..Default::default()
                },
                text: Text::from_sections(
                    [TextSection::new(
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
                    )]
                    .into_iter(),
                ),
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

        let Some(rec) = chatbox.active_chat_sink.as_mut() else { panic!() };
        while let Ok(chat) = rec.try_recv() {
            let msg = make_chat(&mut commands, &asset_server, chat);
            commands.entity(entity).add_child(msg);
        }
    }

    if chatbox.active_tab == "Scene Log" {
        let current_scene = player
            .get_single()
            .map(|player| containing_scene.get(player))
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
        }
    }
}

#[derive(Component)]
struct FpsLabel;

fn update_fps(
    mut q: Query<&mut Text, With<FpsLabel>>,
    diagnostics: Res<Diagnostics>,
    mut last_update: Local<u32>,
    time: Res<Time>,
) {
    let tick = (time.elapsed_seconds() * 10.0) as u32;
    if tick == *last_update {
        return;
    }
    *last_update = tick;

    if let Ok(mut text) = q.get_single_mut() {
        if let Some(fps) = diagnostics.get(FrameTimeDiagnosticsPlugin::FPS) {
            let fps = fps.smoothed().unwrap_or_default();
            text.sections[0].value = format!("fps: {fps:.0}");
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
    mut chat_input: Query<&mut TextBox, With<ChatInput>>,
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

#[derive(Component)]
pub struct ChatButton(&'static str);

fn select_chat_tab(
    In(tab): In<&'static str>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut chatbox: Query<(Entity, &mut ChatBox, &mut Style)>,
    mut chatinput: Query<&mut Style, (With<ChatInput>, Without<ChatBox>)>,
    mut buttons: Query<(&ChatButton, &mut Active)>,
    time: Res<Time>,
) {
    let (entity, mut chatbox, mut style) = chatbox.single_mut();

    let clicked_current = chatbox.active_tab == tab;
    let visible = matches!(style.display, Display::Flex);

    let new_vis = if clicked_current && visible {
        Display::None
    } else {
        Display::Flex
    };

    style.display = new_vis;

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

    for mut style in chatinput.iter_mut() {
        style.display = new_vis;
    }
}

fn update_chatbox_focus(
    mut chat: Query<(&mut BackgroundColor, &Interaction), With<ChatBox>>,
    focused_input: Query<(), (With<ChatInput>, With<Focus>)>,
) {
    let (mut bg, interaction) = chat.single_mut();

    // keep focus if either input has focus, or we are hovering
    if focused_input.get_single().is_ok() || !matches!(interaction, Interaction::None) {
        bg.0 = Color::rgba(0.0, 0.0, 0.25, 0.8);
    } else {
        bg.0 = Color::rgba(0.0, 0.0, 0.25, 0.2);
    }
}
