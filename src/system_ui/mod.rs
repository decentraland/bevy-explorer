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
    dcl_component::proto_components::kernel::comms::rfc4,
    scene_runner::PrimaryUser,
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
        app.add_system(update_fps);
        app.add_system(emit_chat);
        app.add_plugin(EguiPlugin);
        app.add_plugin(UiActionPlugin);
        app.add_plugin(FocusPlugin);
        app.add_plugin(InteractStylePlugin);
        app.add_system(update_textboxes);
    }
}

#[derive(Component)]
pub struct ChatBox {
    tab: &'static str,
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
                                width: Val::Percent(30.0),
                                height: Val::Percent(50.0),
                            },
                            min_size: Size {
                                width: Val::Px(200.0),
                                height: Val::Px(120.0),
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
                    actions.on_hover_enter(
                        |mut chat: Query<&mut BackgroundColor, With<ChatBox>>| {
                            chat.single_mut().0 = Color::rgba(0.0, 0.0, 0.25, 0.8);
                        },
                    ),
                    actions.on_hover_exit(chatbox_defocus),
                    actions.on_click(
                        |mut commands: Commands, q: Query<Entity, With<ChatInput>>| {
                            commands.entity(q.single()).remove::<Focus>().insert(Focus);
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
                            make_button(commands, "Something Else", false);
                        });

                    // chat display
                    commands.spawn((
                        NodeBundle {
                            style: ui::Style {
                                // border: UiRect::all(Val::Px(5.0)),
                                flex_direction: FlexDirection::Column,
                                justify_content: JustifyContent::FlexEnd,
                                size: Size {
                                    width: Val::Percent(100.0),
                                    height: Val::Auto,
                                },
                                flex_grow: 1.0,
                                ..Default::default()
                            },
                            background_color: BackgroundColor(Color::rgba(0.0, 0.0, 0.5, 0.2)),
                            ..Default::default()
                        },
                        ChatBox { tab: "Nearby" },
                        Interaction::default(),
                    ));

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
                        actions.on_defocus(chatbox_defocus),
                    ));
                });
        });
}

#[derive(Component)]
pub struct DisplayChatMessage {
    timestamp: f32,
}

#[allow(clippy::too_many_arguments)]
fn display_chat(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut chats: EventReader<ChatEvent>,
    mut chatbox: Query<(Entity, Option<&Children>), With<ChatBox>>,
    messages: Query<&DisplayChatMessage>,
    users: Query<&UserProfile>,
    time: Res<Time>,
) {
    let (box_ent, maybe_children) = chatbox.single_mut();

    for chat in chats.iter() {
        let Ok(profile) = users.get(chat.sender) else {
            warn!("can't get profile for chat sender {:?}", chat.sender);
            continue;
        };

        println!(
            "chat from {:?} at {}: {}",
            chat.sender, chat.timestamp, chat.message
        );
        let msg = commands
            .spawn((
                DisplayChatMessage {
                    timestamp: time.elapsed_seconds(),
                },
                TextBundle {
                    text: Text::from_sections(
                        [
                            TextSection::new(
                                format!("{}: ", profile.content.name),
                                TextStyle {
                                    font: asset_server.load("fonts/FiraSans-Bold.ttf"),
                                    font_size: 15.0,
                                    color: Color::YELLOW,
                                },
                            ),
                            TextSection::new(
                                chat.message.clone(),
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
            .id();
        commands.entity(box_ent).add_child(msg);
    }

    // clean up old messages
    // TODO fade or something
    if let Some(children) = maybe_children {
        for child in children.iter() {
            if let Ok(message) = messages.get(*child) {
                if message.timestamp + 10.0 < time.elapsed_seconds() {
                    commands.entity(*child).despawn_recursive();
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

fn emit_chat(
    mut chats: EventWriter<ChatEvent>,
    transports: Query<&Transport>,
    player: Query<Entity, With<PrimaryUser>>,
    time: Res<Time>,
    mut chatbox: Query<&mut TextBox, With<ChatInput>>,
) {
    let Ok(player) = player.get_single() else {
        return;
    };
    let Ok(mut textbox) = chatbox.get_single_mut() else {
        return;
    };

    for message in textbox.messages.drain(..) {
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
            message,
        });
    }
}

#[derive(Component)]
pub struct ChatButton(&'static str);

fn select_chat_tab(
    In(tab): In<&'static str>,
    mut chatbox: Query<(&mut ChatBox, &mut Style)>,
    mut chatinput: Query<&mut Style, (With<ChatInput>, Without<ChatBox>)>,
    mut buttons: Query<(&ChatButton, &mut Active)>,
) {
    let (mut chatbox, mut style) = chatbox.single_mut();

    let clicked_current = chatbox.tab == tab;
    let visible = matches!(style.display, Display::Flex);

    let new_vis = if clicked_current && visible {
        Display::None
    } else {
        Display::Flex
    };

    style.display = new_vis;

    if !clicked_current {
        println!("need to toggle here ...");
        chatbox.tab = tab;
    }

    for (button, mut active) in buttons.iter_mut() {
        if button.0 == tab {
            println!("{} -> active ({})", button.0, !(clicked_current && visible));
            active.0 = !(clicked_current && visible);
        } else {
            println!("{} -> not active", button.0);
            active.0 = false;
        }
    }

    for mut style in chatinput.iter_mut() {
        style.display = new_vis;
    }
}

fn chatbox_defocus(
    mut chat: Query<(&mut BackgroundColor, &Interaction), With<ChatBox>>,
    focused_input: Query<(), (With<ChatInput>, With<Focus>)>,
) {
    println!("container hover exit");

    let (mut bg, interaction) = chat.single_mut();

    // keep focus if either input has focus, or we are hovering
    if focused_input.get_single().is_ok() || !matches!(interaction, Interaction::None) {
        println!("no defocus");
        return;
    }

    bg.0 = Color::rgba(0.0, 0.0, 0.25, 0.2);
}

trait Unit {
    fn unit(&self) {}
}

impl<T> Unit for T {}
