// todo
// - separate js crate
// - budget -> deadline is just last end + frame time

mod camera_controller;
pub mod dcl;
mod dcl_component;
mod input_handler;
mod ipfs;
mod scene_runner;

use bevy::{
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    pbr::CascadeShadowConfigBuilder,
    prelude::*,
};

use bevy_prototype_debug_lines::DebugLinesPlugin;
use camera_controller::CameraController;
use dcl::SceneDefinition;
use scene_runner::{LoadSceneEvent, PrimaryCamera, RendererSceneContext, SceneRunnerPlugin};

use crate::{camera_controller::CameraControllerPlugin, scene_runner::SceneSets};

#[derive(Resource)]
struct UserScriptFolder(String);

const LOG_FPS: bool = true;

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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let user_script_folder = args.get(1).expect("please enter script folder");

    let mut app = App::new();

    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            present_mode: bevy::window::PresentMode::Immediate,
            ..Default::default()
        }),
        ..Default::default()
    }))
    .add_plugin(DebugLinesPlugin::with_depth_test(true))
    .add_plugin(SceneRunnerPlugin) // script engine plugin
    .add_plugin(CameraControllerPlugin)
    .add_startup_system(setup)
    .insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 0.1,
    })
    .insert_resource(UserScriptFolder(user_script_folder.clone()));

    if LOG_FPS {
        app.add_plugin(FrameTimeDiagnosticsPlugin::default())
            .add_plugin(LogDiagnosticsPlugin::default());
    }

    app.add_system(input.after(SceneSets::RunLoop));
    println!("up: increase scene count, down: decrease scene count");

    app.run()
}

fn setup(
    mut commands: Commands,
    mut scene_load: EventWriter<LoadSceneEvent>,
    user_script_folder: Res<UserScriptFolder>,
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
    for i in 0..1 {
        scene_load.send(LoadSceneEvent {
            scene: SceneDefinition {
                path: user_script_folder.0.clone(),
                offset: Vec3::X * 2.0 * i as f32,
                visible: i % 10 == 0,
            },
        });
    }
}

fn input(
    keys: Res<Input<KeyCode>>,
    mut load: EventWriter<LoadSceneEvent>,
    mut commands: Commands,
    scenes: Query<Entity, With<RendererSceneContext>>,
    user_script_folder: Res<UserScriptFolder>,
) {
    if keys.pressed(KeyCode::Up) {
        let count = scenes.iter().count();
        load.send(LoadSceneEvent {
            scene: SceneDefinition {
                path: user_script_folder.0.clone(),
                offset: Vec3::X * 16.0 * count as f32,
                visible: count.count_ones() <= 1,
            },
        });
        println!("+ -> {}", count + 1);
    }

    if keys.pressed(KeyCode::Down) {
        let count = scenes.iter().count();
        if let Some(entity) = scenes.iter().last() {
            commands.entity(entity).despawn_recursive();
            println!("- -> {}", count - 1);
        }
    }
}
