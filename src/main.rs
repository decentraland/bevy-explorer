// todo
// - separate js crate
// - budget -> deadline is just last end + frame time

mod camera_controller;
pub mod dcl;
mod dcl_component;
mod input_handler;
pub mod ipfs;
mod scene_runner;

use bevy::{
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    pbr::CascadeShadowConfigBuilder,
    prelude::*,
};

use bevy_prototype_debug_lines::DebugLinesPlugin;
use camera_controller::CameraController;
use ipfs::SceneIpfsLocation;
use scene_runner::{LoadSceneEvent, PrimaryCamera, SceneRunnerPlugin};
use serde::{Deserialize, Serialize};

use crate::{
    camera_controller::CameraControllerPlugin, ipfs::IpfsIoPlugin, scene_runner::SceneSets,
};

#[derive(Resource)]
struct InitialLocation(SceneIpfsLocation);

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

#[derive(Serialize, Deserialize)]
pub struct AppConfig {
    server: String,
    scene: SceneIpfsLocation,
    graphics: GraphicsSettings,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: "https://sdk-test-scenes.decentraland.zone".to_owned(),
            scene: SceneIpfsLocation::Pointer(0, 0),
            graphics: Default::default(),
        }
    }
}

fn parse_scene_location(scene: &str) -> Result<SceneIpfsLocation, anyhow::Error> {
    if scene.ends_with(".js") {
        return Ok(SceneIpfsLocation::Js(scene[0..scene.len() - 3].to_string()));
    }

    if let Some((px, py)) = scene.split_once(',') {
        return Ok(SceneIpfsLocation::Pointer(
            px.parse::<i32>()?,
            py.parse::<i32>()?,
        ));
    };

    Ok(SceneIpfsLocation::Hash(scene.to_owned()))
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
            .unwrap()
            .unwrap_or(base_config.scene),
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
                    present_mode,
                    ..Default::default()
                }),
                ..Default::default()
            })
            .build()
            .add_before::<bevy::asset::AssetPlugin, _>(IpfsIoPlugin {
                server_prefix: final_config.server,
            }),
    )
    .add_plugin(DebugLinesPlugin::with_depth_test(true))
    .add_plugin(SceneRunnerPlugin) // script engine plugin
    .add_plugin(CameraControllerPlugin)
    .add_startup_system(setup)
    .insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 0.1,
    })
    .insert_resource(InitialLocation(final_config.scene));

    if final_config.graphics.log_fps {
        app.add_plugin(FrameTimeDiagnosticsPlugin::default())
            .add_plugin(LogDiagnosticsPlugin::default());
    }

    app.add_system(input.after(SceneSets::RunLoop));
    println!("up: increase scene count, down: decrease scene count");

    // replay any warnings
    for warning in warnings {
        warn!(warning);
    }

    app.run()
}

fn setup(
    mut commands: Commands,
    mut scene_load: EventWriter<LoadSceneEvent>,
    initial_location: Res<InitialLocation>,
) {
    // add a camera
    commands.spawn((
        Camera3dBundle {
            transform: Transform::from_translation(Vec3::new(-10.0, 10.0, 4.0))
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
            maximum_distance: 20.0,
            ..Default::default()
        }
        .into(),
        ..Default::default()
    });

    // load the scene
    info!("loading scene: {:?}", initial_location.0);
    scene_load.send(LoadSceneEvent {
        location: initial_location.0.clone(),
    });
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
