use bevy::{
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    math::Vec3Swizzles,
    prelude::*,
    ui::FocusPolicy,
};

use bevy_console::ConsoleCommand;
use bevy_dui::{DuiCommandsExt, DuiEntities, DuiRegistry};
use common::{
    sets::SetupSets,
    structs::{AppConfig, PrimaryUser},
};
use comms::{global_crdt::ForeignPlayer, Transport};
use console::DoAddConsoleCommand;
use scene_runner::{
    initialize_scene::{SceneLoading, PARCEL_SIZE},
    renderer_context::RendererSceneContext,
    ContainingScene, DebugInfo,
};
use ui_core::{
    ui_actions::{Click, EventCloneExt},
    BODY_TEXT_STYLE, TITLE_TEXT_STYLE,
};

use crate::{
    map::MapTexture,
    profile::{SettingsTab, ShowSettingsEvent},
};

use super::SystemUiRoot;

pub struct SysInfoPanelPlugin;

impl Plugin for SysInfoPanelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            setup.in_set(SetupSets::Main).after(SetupSets::Init),
        );
        app.add_systems(Update, (update_scene_load_state, update_minimap));
        app.add_systems(
            OnEnter::<ui_core::State>(ui_core::State::Ready),
            setup_minimap,
        );
        app.add_console_command::<SysinfoCommand, _>(set_sysinfo);
    }
}

#[derive(Component)]
struct SysInfoMarker;

#[derive(Component)]
struct SysInfoContainer;

pub(crate) fn setup(
    mut commands: Commands,
    root: Res<SystemUiRoot>,
    config: Res<AppConfig>,
    asset_server: Res<AssetServer>,
) {
    commands.entity(root.0).with_children(|commands| {
        commands
            .spawn(NodeBundle {
                style: Style {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(50.0),
                    top: Val::Percent(50.0),
                    right: Val::Percent(50.0),
                    bottom: Val::Percent(50.0),
                    align_content: AlignContent::Center,
                    justify_content: JustifyContent::Center,
                    ..Default::default()
                },
                ..Default::default()
            })
            .with_children(|c| {
                c.spawn(ImageBundle {
                    style: Style {
                        width: Val::VMin(3.0),
                        height: Val::VMin(3.0),
                        ..Default::default()
                    },
                    image: asset_server.load("images/crosshair.png").into(),
                    background_color: Color::rgba(1.0, 1.0, 1.0, 0.7).into(),
                    ..Default::default()
                });
            });
    });

    commands.entity(root.0).with_children(|commands| {
        commands
            .spawn((
                NodeBundle {
                    style: Style {
                        flex_direction: FlexDirection::Column,
                        align_self: AlignSelf::FlexStart,
                        border: UiRect::all(Val::Px(5.)),
                        ..default()
                    },
                    visibility: if config.sysinfo_visible {
                        Visibility::Visible
                    } else {
                        Visibility::Hidden
                    },
                    background_color: Color::rgba(0.8, 0.8, 1.0, 0.8).into(),
                    focus_policy: FocusPolicy::Block,
                    z_index: ZIndex::Global(100),
                    ..default()
                },
                SysInfoContainer,
            ))
            .with_children(|commands| {
                commands.spawn(TextBundle::from_section(
                    "System Info",
                    TITLE_TEXT_STYLE.get().unwrap().clone(),
                ));

                commands
                    .spawn((
                        NodeBundle {
                            style: Style {
                                flex_direction: FlexDirection::Column,
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                        SysInfoMarker,
                    ))
                    .with_children(|commands| {
                        let mut info_node = |label: String| {
                            commands
                                .spawn(NodeBundle::default())
                                .with_children(|commands| {
                                    commands.spawn(TextBundle {
                                        style: Style {
                                            width: Val::Px(120.0),
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
                                            width: Val::Px(120.0),
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

                        if config.graphics.log_fps {
                            info_node("FPS :".to_owned());
                        }

                        info_node("Current Parcel :".to_owned());
                        info_node("Current Scene :".to_owned());
                        info_node("Scene State :".to_owned());
                        info_node("Loading Scenes :".to_owned());
                        info_node("Running Scenes :".to_owned());
                        info_node("Blocked Scenes :".to_owned());
                        info_node("Broken Scenes :".to_owned());
                        info_node("Transports :".to_owned());
                        info_node("Players :".to_owned());
                        info_node("Debug info :".to_owned());
                    });
            });
    });
}

#[derive(Component)]
pub struct Minimap;

#[allow(clippy::too_many_arguments)]
fn update_scene_load_state(
    q: Query<Entity, With<SysInfoMarker>>,
    q_children: Query<&Children>,
    mut text: Query<&mut Text>,
    mut style: Query<&mut Style>,
    loading_scenes: Query<&SceneLoading>,
    running_scenes: Query<&RendererSceneContext, Without<SceneLoading>>,
    transports: Query<&Transport>,
    players: Query<&ForeignPlayer>,
    config: Res<AppConfig>,
    mut last_update: Local<u32>,
    time: Res<Time>,
    diagnostics: Res<DiagnosticsStore>,
    containing_scene: ContainingScene,
    player: Query<(Entity, &GlobalTransform), With<PrimaryUser>>,
    debug_info: Res<DebugInfo>,
) {
    let tick = (time.elapsed_seconds() * 10.0) as u32;
    if tick == *last_update {
        return;
    }
    *last_update = tick;

    let Ok((player, pos)) = player.get_single() else {
        return;
    };
    let scene = containing_scene.get_parcel(player);
    let parcel = (pos.translation().xz() * Vec2::new(1.0, -1.0) / PARCEL_SIZE)
        .floor()
        .as_ivec2();
    let title = scene
        .and_then(|scene| running_scenes.get(scene).ok())
        .map(|context| context.title.clone())
        .unwrap_or("???".to_owned());

    if let Ok(sysinfo) = q.get_single() {
        let mut ix = 0;
        let children = q_children.get(sysinfo).unwrap();
        let mut set_child = |value: String| {
            if value.is_empty() {
                style.get_mut(children[ix]).unwrap().display = Display::None;
            } else {
                style.get_mut(children[ix]).unwrap().display = Display::Flex;
            }
            let container = q_children.get(children[ix]).unwrap();
            let mut text = text.get_mut(container[1]).unwrap();
            text.sections[0].value = value;
            ix += 1;
        };

        let state = scene.map_or("-", |scene| {
            if loading_scenes.get(scene).is_ok() {
                "Loading"
            } else if let Ok(scene) = running_scenes.get(scene) {
                if scene.broken {
                    "Broken"
                } else if !scene.blocked.is_empty() {
                    "Blocked"
                } else {
                    "Running"
                }
            } else {
                "Unknown?!"
            }
        });

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

        if config.graphics.log_fps {
            if let Some(fps) = diagnostics.get(FrameTimeDiagnosticsPlugin::FPS) {
                let fps = fps.smoothed().unwrap_or_default();
                set_child(format!("{fps:.0}"));
            } else {
                set_child("-".to_owned());
            }
        }

        set_child(format!("({},{})", parcel.x, parcel.y));
        set_child(title);
        set_child(state.to_owned());

        set_child(format!("{}", loading));
        set_child(format!("{}", running));
        set_child(format!("{}", blocked));
        set_child(format!("{}", broken));

        set_child(format!("{}", transports));
        set_child(format!("{}", players));

        let debug_info = debug_info
            .info
            .iter()
            .fold(String::default(), |msg, (key, info)| {
                format!("{msg}\n{key}: {info}")
            });
        set_child(debug_info);
    }
}

fn setup_minimap(mut commands: Commands, root: Res<SystemUiRoot>, dui: Res<DuiRegistry>) {
    let components = commands
        .entity(root.0)
        .spawn_template(&dui, "minimap", Default::default())
        .unwrap();
    commands.entity(components.root).insert(Minimap);
    commands.entity(components.named("map-node")).insert((
        MapTexture {
            center: Default::default(),
            parcels_per_vmin: 100.0,
            icon_min_size_vmin: 0.03,
        },
        Interaction::default(),
        ShowSettingsEvent(SettingsTab::Map).send_value_on::<Click>(),
    ));
}

fn update_minimap(
    q: Query<&DuiEntities, With<Minimap>>,
    mut maps: Query<&mut MapTexture>,
    player: Query<(Entity, &GlobalTransform), With<PrimaryUser>>,
    containing_scene: ContainingScene,
    scenes: Query<&RendererSceneContext>,
    mut text: Query<&mut Text>,
) {
    let Ok((player, gt)) = player.get_single() else {
        return;
    };

    let player_translation = (gt.translation().xz() * Vec2::new(1.0, -1.0)) / PARCEL_SIZE;
    let map_center = player_translation - Vec2::Y; // no idea why i have to subtract one :(

    let scene = containing_scene.get_parcel(player);
    let parcel = player_translation.floor().as_ivec2();
    let title = scene
        .and_then(|scene| scenes.get(scene).ok())
        .map(|context| context.title.clone())
        .unwrap_or("???".to_owned());

    if let Ok(components) = q.get_single() {
        if let Ok(mut map) = maps.get_mut(components.named("map-node")) {
            map.center = map_center;
        }

        if let Ok(mut text) = text.get_mut(components.named("title")) {
            text.sections[0].value = title;
        }

        if let Ok(mut text) = text.get_mut(components.named("position")) {
            text.sections[0].value = format!("({},{})", parcel.x, parcel.y);
        }
    }
}

// set fps
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/sysinfo")]
struct SysinfoCommand {
    on: Option<bool>,
}

fn set_sysinfo(
    mut commands: Commands,
    mut input: ConsoleCommand<SysinfoCommand>,
    q: Query<Entity, With<SysInfoContainer>>,
) {
    if let Some(Ok(command)) = input.take() {
        let on = command.on.unwrap_or(true);

        commands.entity(q.single()).insert(if on {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
    }
}
