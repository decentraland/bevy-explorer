// todo
// - separate js crate
// - budget -> deadline is just last end + frame time

mod camera_controller;
pub mod console;
pub mod dcl;
pub mod dcl_component;
pub mod input_handler;
pub mod ipfs;
pub mod scene_runner;
pub mod visuals;

use bevy::{
    core::FrameCount,
    core_pipeline::tonemapping::{DebandDither, Tonemapping},
    diagnostic::{Diagnostics, FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    pbr::CascadeShadowConfigBuilder,
    prelude::*,
    render::view::ColorGrading,
};

use bevy_console::ConsoleOpen;
use bevy_prototype_debug_lines::DebugLinesPlugin;
use camera_controller::CameraController;
use ipfs::ChangeRealmEvent;
use scene_runner::{
    initialize_scene::SceneLoading, renderer_context::RendererSceneContext, PrimaryCamera,
    SceneRunnerPlugin,
};
use serde::{Deserialize, Serialize};

use crate::{
    camera_controller::CameraControllerPlugin, console::ConsolePlugin, ipfs::IpfsIoPlugin,
    scene_runner::SceneSets, visuals::VisualsPlugin,
};

// macro for assertions
// by default, enabled in debug builds and disabled in release builds
// can be enabled for release with `cargo run --release --features="dcl-assert"`
#[cfg(any(debug_assertions, feature = "dcl-assert"))]
#[macro_export]
macro_rules! dcl_assert {
    ($($arg:tt)*) => ( assert!($($arg)*); )
}
#[cfg(not(any(debug_assertions, feature = "dcl-assert")))]
#[macro_export]
macro_rules! dcl_assert {
    ($($arg:tt)*) => {};
}

#[derive(Serialize, Deserialize)]
pub struct GraphicsSettings {
    vsync: bool,
    log_fps: bool,
}

impl Default for GraphicsSettings {
    fn default() -> Self {
        Self {
            vsync: true,
            log_fps: true,
        }
    }
}

#[derive(Serialize, Deserialize, Resource)]
pub struct AppConfig {
    server: String,
    graphics: GraphicsSettings,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: "https://sdk-test-scenes.decentraland.zone".to_owned(),
            graphics: Default::default(),
        }
    }
}

fn main() {
    // warnings before log init must be stored and replayed later
    let mut warnings = Vec::default();

    let base_config: AppConfig = std::fs::read("config.json")
        .ok()
        .and_then(|f| {
            serde_json::from_slice(&f)
                .map_err(|e| warnings.push(format!("failed to parse config.json: {e}")))
                .ok()
        })
        .unwrap_or(Default::default());
    let mut args = pico_args::Arguments::from_env();

    let final_config = AppConfig {
        server: args
            .value_from_str("--server")
            .ok()
            .unwrap_or(base_config.server),
        graphics: GraphicsSettings {
            vsync: args
                .value_from_str("--vsync")
                .ok()
                .unwrap_or(base_config.graphics.vsync),
            log_fps: args
                .value_from_str("--log_fps")
                .ok()
                .unwrap_or(base_config.graphics.log_fps),
        },
    };

    let remaining = args.finish();
    if !remaining.is_empty() {
        println!(
            "failed to parse args: {}",
            remaining
                .iter()
                .map(|arg| arg.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ")
        );
        return;
    }

    // std::fs::write(
    //     "config.json.out",
    //     serde_json::to_string(&final_config).unwrap(),
    // )
    // .unwrap();

    let mut app = App::new();
    let present_mode = match final_config.graphics.vsync {
        true => bevy::window::PresentMode::AutoVsync,
        false => bevy::window::PresentMode::AutoNoVsync,
    };

    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Decentraland Bevy Explorer".to_owned(),
                    present_mode,
                    ..Default::default()
                }),
                ..Default::default()
            })
            .set(bevy::log::LogPlugin {
                filter: "wgpu=error,bevy_animation=error".to_string(),
                ..default()
            })
            .build()
            .add_before::<bevy::asset::AssetPlugin, _>(IpfsIoPlugin {
                starting_realm: Some(final_config.server.clone()),
                cache_root: Default::default(),
            }),
    )
    .add_plugin(DebugLinesPlugin::with_depth_test(true))
    .add_plugin(SceneRunnerPlugin) // script engine plugin
    .add_plugin(CameraControllerPlugin)
    .add_plugin(ConsolePlugin)
    .add_plugin(VisualsPlugin)
    .add_startup_system(setup)
    .insert_resource(AmbientLight {
        color: Color::rgb(0.5, 0.5, 1.0),
        brightness: 0.25,
    });

    if final_config.graphics.log_fps {
        app.add_plugin(FrameTimeDiagnosticsPlugin::default())
            .add_plugin(LogDiagnosticsPlugin::default());

        app.add_system(update_fps);
    }

    app.add_system(
        input
            .after(SceneSets::RunLoop)
            .run_if(|console_open: Res<ConsoleOpen>| !console_open.open),
    );
    println!("up: realm1, down: realm2");

    // replay any warnings
    for warning in warnings {
        warn!(warning);
    }

    app.insert_resource(final_config);

    app.run()
}

fn setup(mut commands: Commands, config: Res<AppConfig>, asset_server: Res<AssetServer>) {
    // add a camera
    commands.spawn((
        Camera3dBundle {
            transform: Transform::from_translation(Vec3::new(16.0 * 77.5, 2.0, 16.0 * 7.5))
                .looking_at(Vec3::new(1.0, 8.0, -1.0), Vec3::Y),
            tonemapping: Tonemapping::TonyMcMapface,
            dither: DebandDither::Enabled,
            color_grading: ColorGrading {
                exposure: -0.5,
                gamma: 1.5,
                pre_saturation: 1.0,
                post_saturation: 1.0,
            },
            ..Default::default()
        },
        PrimaryCamera,
        CameraController::default(),
    ));

    // add a directional light so it looks nicer
    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
            color: Color::rgb(1.0, 1.0, 0.7),
            shadows_enabled: true,
            ..Default::default()
        },
        transform: Transform::default().looking_at(Vec3::new(0.2, -0.5, -1.0), Vec3::Y),
        cascade_shadow_config: CascadeShadowConfigBuilder {
            maximum_distance: 100.0,
            ..Default::default()
        }
        .into(),
        ..Default::default()
    });

    // fps counter
    if config.graphics.log_fps {
        commands
            .spawn(NodeBundle {
                style: Style {
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

fn input(
    keys: Res<Input<KeyCode>>,
    mut load: EventWriter<ChangeRealmEvent>,
    frame: Res<FrameCount>,
    loading_scenes: Query<(), With<SceneLoading>>,
    running_scenes: Query<(), With<RendererSceneContext>>,
) {
    let realm = if keys.pressed(KeyCode::Up) {
        "https://sdk-test-scenes.decentraland.zone"
    } else if keys.pressed(KeyCode::Down) {
        "https://sdk-team-cdn.decentraland.org/ipfs/goerli-plaza-23c44f78405b2ee2e063a808d3b031905bc59800"
    } else {
        ""
    };

    if !realm.is_empty() {
        load.send(ChangeRealmEvent {
            new_realm: realm.to_owned(),
        });
    }

    if frame.0 % 1000 == 0 {
        info!("{} loading", loading_scenes.iter().count());
        info!("{} running", running_scenes.iter().count());
    }
}

// hook console commands
#[cfg(not(test))]
impl console::DoAddConsoleCommand for App {
    fn add_console_command<T: bevy_console::Command, U>(
        &mut self,
        system: impl IntoSystemConfig<U>,
    ) -> &mut Self {
        bevy_console::AddConsoleCommand::add_console_command::<T, U>(self, system)
    }
}
