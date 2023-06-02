use bevy::{
    diagnostic::{Diagnostics, FrameTimeDiagnosticsPlugin},
    prelude::*,
};

use crate::{
    comms::{global_crdt::ForeignPlayer, Transport},
    scene_runner::{initialize_scene::SceneLoading, renderer_context::RendererSceneContext},
    AppConfig,
};

use super::{SystemUiRoot, BODY_TEXT_STYLE, TITLE_TEXT_STYLE};

pub struct SysInfoPlanelPlugin;

impl Plugin for SysInfoPlanelPlugin {
    fn build(&self, app: &mut App) {
        app.add_startup_system(setup.after(super::setup));
        app.add_system(update_fps);
        app.add_system(update_scene_load_state);
    }
}

#[derive(Component)]
struct FpsLabel;

#[derive(Component)]
struct SceneLoadLabel;

fn setup(mut commands: Commands, root: Res<SystemUiRoot>, config: Res<AppConfig>) {
    commands.entity(root.0).with_children(|commands| {
        commands
            .spawn(NodeBundle {
                style: Style {
                    flex_direction: FlexDirection::Column,
                    align_self: AlignSelf::FlexStart,
                    border: UiRect::all(Val::Px(5.)),
                    ..default()
                },
                background_color: Color::rgba(0.8, 0.8, 1.0, 0.8).into(),
                ..default()
            })
            .with_children(|commands| {
                commands.spawn(TextBundle::from_section(
                    "System Info",
                    TITLE_TEXT_STYLE.get().unwrap().clone(),
                ));

                // fps counter
                if config.graphics.log_fps {
                    commands.spawn((
                        TextBundle::from_section("FPS", BODY_TEXT_STYLE.get().unwrap().clone())
                            .with_style(Style {
                                margin: UiRect::all(Val::Px(5.)),
                                ..default()
                            }),
                        FpsLabel,
                    ));
                }

                commands
                    .spawn((
                        NodeBundle {
                            style: Style {
                                flex_direction: FlexDirection::Column,
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                        SceneLoadLabel,
                    ))
                    .with_children(|commands| {
                        let mut info_node = |label: String| {
                            commands
                                .spawn(NodeBundle::default())
                                .with_children(|commands| {
                                    commands.spawn(TextBundle {
                                        style: Style {
                                            size: Size::width(Val::Px(100.0)),
                                            ..Default::default()
                                        },
                                        text: Text::from_section(
                                            label,
                                            BODY_TEXT_STYLE.get().unwrap().clone(),
                                        )
                                        .with_alignment(TextAlignment::Right),
                                        ..Default::default()
                                    });
                                    commands.spawn(TextBundle {
                                        style: Style {
                                            size: Size::width(Val::Px(100.0)),
                                            ..Default::default()
                                        },
                                        text: Text::from_section(
                                            "",
                                            BODY_TEXT_STYLE.get().unwrap().clone(),
                                        ),
                                        ..Default::default()
                                    });
                                });
                        };

                        info_node("Loading Scenes".to_owned());
                        info_node("Running Scenes".to_owned());
                        info_node("Blocked Scenes".to_owned());
                        info_node("Broken Scenes".to_owned());
                        info_node("Transports".to_owned());
                        info_node("Players".to_owned());
                    });
            });
    });
}

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

fn update_scene_load_state(
    q: Query<Entity, With<SceneLoadLabel>>,
    q_children: Query<&Children>,
    mut t: Query<&mut Text>,
    loading_scenes: Query<&SceneLoading>,
    running_scenes: Query<&RendererSceneContext>,
    transports: Query<&Transport>,
    players: Query<&ForeignPlayer>,
) {
    if let Ok(sysinfo) = q.get_single() {
        let children = q_children.get(sysinfo).unwrap();
        let mut set_child = |ix: usize, value: String| {
            let container = q_children.get(children[ix]).unwrap();
            let mut text = t.get_mut(container[1]).unwrap();
            text.sections[0].value = value;
        };

        let loading = loading_scenes.iter().count();
        let running = running_scenes
            .iter()
            .filter(|context| !context.broken && context.blocked.is_empty())
            .count();
        let blocked = running_scenes
            .iter()
            .filter(|context| !context.broken && !context.blocked.is_empty())
            .count();
        let broken = running_scenes
            .iter()
            .filter(|context| context.broken)
            .count();
        let transports = transports.iter().count();
        let players = players.iter().count() + 1;

        set_child(0, format!("{}", loading));
        set_child(1, format!("{}", running));
        set_child(2, format!("{}", blocked));
        set_child(3, format!("{}", broken));

        set_child(4, format!("{}", transports));
        set_child(5, format!("{}", players));
    }
}
