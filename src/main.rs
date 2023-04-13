// todo
// - separate js crate
// - budget -> deadline is just last end + frame time

mod camera_controller;
pub mod dcl;
mod dcl_component;
mod input_handler;
pub mod ipfs;
mod scene_runner;

use std::path::Path;

use bevy::{
    diagnostic::{Diagnostics, FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    pbr::CascadeShadowConfigBuilder,
    prelude::*,
};

use bevy_prototype_debug_lines::DebugLinesPlugin;
use camera_controller::CameraController;
use ipfs::{ipfs_path::IpfsPath, SceneIpfsLocation};
use scene_runner::{LoadSceneEvent, PrimaryCamera, SceneRunnerPlugin};
use serde::{Deserialize, Serialize};

use crate::{
    camera_controller::CameraControllerPlugin, ipfs::IpfsIoPlugin, scene_runner::SceneSets,
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
    scene: Option<SceneIpfsLocation>,
    graphics: GraphicsSettings,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: "https://sdk-test-scenes.decentraland.zone".to_owned(),
            scene: None,
            graphics: Default::default(),
        }
    }
}

fn parse_scene_location(scene: &str) -> Result<SceneIpfsLocation, anyhow::Error> {
    Err(anyhow::anyhow!("nope"))
    // if scene.ends_with(".js") {
    //     return Ok(SceneIpfsLocation::Js(scene[0..scene.len() - 3].to_string()));
    // }

    // if let Some((px, py)) = scene.split_once(',') {
    //     return Ok(SceneIpfsLocation::Pointer(
    //         px.parse::<i32>()?,
    //         py.parse::<i32>()?,
    //     ));
    // };

    // Ok(SceneIpfsLocation::IpfsPath(
    //     IpfsPath::new_from_path(Path::new(scene))?.unwrap(),
    // ))
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
        scene: args
            .opt_value_from_fn("--scene", parse_scene_location)
            .unwrap(),
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
            }),
    )
    .add_plugin(DebugLinesPlugin::with_depth_test(true))
    .add_plugin(SceneRunnerPlugin {
        dynamic_spawning: final_config.scene.is_none(),
    }) // script engine plugin
    .add_plugin(CameraControllerPlugin)
    .add_startup_system(setup)
    .insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 0.1,
    });

    if final_config.graphics.log_fps {
        app.add_plugin(FrameTimeDiagnosticsPlugin::default())
            .add_plugin(LogDiagnosticsPlugin::default());

        app.add_system(update_fps);
    }

    app.add_system(input.after(SceneSets::RunLoop));
    println!("up: increase scene count, down: decrease scene count");

    // replay any warnings
    for warning in warnings {
        warn!(warning);
    }

    app.insert_resource(final_config);

    app.run()
}

fn setup(
    mut commands: Commands,
    mut scene_load: EventWriter<LoadSceneEvent>,
    config: Res<AppConfig>,
    asset_server: Res<AssetServer>,
) {
    // add a camera
    commands.spawn((
        Camera3dBundle {
            transform: Transform::from_translation(Vec3::new(16.0 * 77.5, 10.0, 16.0 * 7.5))
                .looking_at(Vec3::new(1.0, 8.0, -1.0), Vec3::Y),
            ..Default::default()
        },
        PrimaryCamera,
        CameraController::default(),
    ));

    // add a directional light so it looks nicer
    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
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

    // load the scene
    if let Some(initial_scene) = config.scene.as_ref() {
        info!("loading scene: {:?}", initial_scene);
        scene_load.send(LoadSceneEvent {
            entity: None,
            location: initial_scene.clone(),
        });
    }

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

fn input(// keys: Res<Input<KeyCode>>,
    // mut load: EventWriter<LoadSceneEvent>,
    // mut commands: Commands,
    // scenes: Query<Entity, With<RendererSceneContext>>,
    // user_script_folder: Res<UserScriptFolder>,
) {
    // if keys.pressed(KeyCode::Up) {
    //     let count = scenes.iter().count();
    //     load.send(LoadSceneEvent {
    //         scene: SceneDefinition {
    //             path: user_script_folder.0.clone(),
    //             offset: Vec3::X * 16.0 * count as f32,
    //             visible: count.count_ones() <= 1,
    //         },
    //     });
    //     println!("+ -> {}", count + 1);
    // }

    // if keys.pressed(KeyCode::Down) {
    //     let count = scenes.iter().count();
    //     if let Some(entity) = scenes.iter().last() {
    //         commands.entity(entity).despawn_recursive();
    //         println!("- -> {}", count - 1);
    //     }
    // }
}
