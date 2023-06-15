use bevy::{
    diagnostic::{Diagnostics, FrameTimeDiagnosticsPlugin},
    math::Vec3Swizzles,
    prelude::*,
};

use common::{structs::{AppConfig, PrimaryUser}, sets::SetupSets};
use comms::{global_crdt::ForeignPlayer, Transport};
use scene_runner::{
    initialize_scene::{SceneLoading, PARCEL_SIZE},
    renderer_context::RendererSceneContext,
    ContainingScene,
};
use ui_core::{BODY_TEXT_STYLE, TITLE_TEXT_STYLE};

use super::SystemUiRoot;

pub struct SysInfoPlanelPlugin;

impl Plugin for SysInfoPlanelPlugin {
    fn build(&self, app: &mut App) {
        app.add_startup_system(setup.in_set(SetupSets::Main).after(SetupSets::Init));
        app.add_system(update_scene_load_state);
    }
}

#[derive(Component)]
struct SysInfoMarker;

pub(crate) fn setup(mut commands: Commands, root: Res<SystemUiRoot>, config: Res<AppConfig>) {
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
                                            size: Size::width(Val::Px(120.0)),
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
                                            size: Size::width(Val::Px(120.0)),
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
                    });
            });
    });
}

#[allow(clippy::too_many_arguments)]
fn update_scene_load_state(
    q: Query<Entity, With<SysInfoMarker>>,
    q_children: Query<&Children>,
    mut t: Query<&mut Text>,
    loading_scenes: Query<&SceneLoading>,
    running_scenes: Query<&RendererSceneContext, Without<SceneLoading>>,
    transports: Query<&Transport>,
    players: Query<&ForeignPlayer>,
    config: Res<AppConfig>,
    mut last_update: Local<u32>,
    time: Res<Time>,
    diagnostics: Res<Diagnostics>,
    containing_scene: ContainingScene,
    player: Query<(Entity, &GlobalTransform), With<PrimaryUser>>,
) {
    let tick = (time.elapsed_seconds() * 10.0) as u32;
    if tick == *last_update {
        return;
    }
    *last_update = tick;

    let Ok((player, pos)) = player.get_single() else {
        return;
    };
    let scene = containing_scene.get(player);
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
            let container = q_children.get(children[ix]).unwrap();
            let mut text = t.get_mut(container[1]).unwrap();
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
    }
}
