pub mod textbox;

use bevy::ui;
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

use self::textbox::{update_textboxes, TextBox};

pub struct SystemUiPlugin;

impl Plugin for SystemUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_startup_system(setup);
        app.add_system(display_chat);
        app.add_system(update_fps);
        app.add_system(emit_chat);
        app.add_plugin(EguiPlugin);
        app.add_system(update_textboxes);
    }
}

#[derive(Component)]
pub struct ChatBox;

fn setup(mut commands: Commands, asset_server: Res<AssetServer>, config: Res<AppConfig>) {
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
            // background_color: BackgroundColor(Color::rgba(0.0, 1.0, 0.0, 0.2)),
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
            commands.spawn((
                NodeBundle {
                    style: ui::Style {
                        border: UiRect::all(Val::Px(5.0)),
                        flex_direction: FlexDirection::Column,
                        justify_content: JustifyContent::FlexEnd,
                        size: Size {
                            width: Val::Percent(100.0),
                            height: Val::Auto,
                        },
                        max_size: Size {
                            width: Val::Percent(100.0),
                            height: Val::Percent(50.0),
                        },
                        ..Default::default()
                    },
                    background_color: BackgroundColor(Color::rgba(0.0, 0.0, 1.0, 0.2)),
                    ..Default::default()
                },
                ChatBox,
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
                            height: Val::Px(30.0),
                        },
                        ..Default::default()
                    },
                    background_color: BackgroundColor(Color::rgba(1.0, 0.0, 1.0, 0.2)),
                    ..Default::default()
                },
                TextBox::default(),
                ChatInput,
            ));
        });
}

#[derive(Component)]
pub struct DisplayChatMessage {
    timestamp: f32,
}

fn display_chat(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut chats: EventReader<ChatEvent>,
    chatbox: Query<(Entity, Option<&Children>), With<ChatBox>>,
    messages: Query<&DisplayChatMessage>,
    users: Query<&UserProfile>,
    time: Res<Time>,
) {
    let (box_ent, maybe_children) = chatbox.single();

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
