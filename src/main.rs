mod input_handler;
mod output_handler;
mod scene_runner;
mod test;

use bevy::{
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    pbr::CascadeShadowConfigBuilder,
    prelude::*,
};

use input_handler::SceneInputPlugin;
use output_handler::SceneOutputPlugin;
use scene_runner::{LoadJsSceneEvent, SceneDefinition, SceneRunnerPlugin};

#[derive(Resource)]
struct UserScriptFolder(String);

const LOG_FPS: bool = true;

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
    .add_plugin(SceneRunnerPlugin) // script engine plugin
    .add_plugin(SceneInputPlugin) // plugin for posting input events to the script
    .add_plugin(SceneOutputPlugin) // plugin for processing some commands from the script
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

    app.run()
}

fn setup(
    mut commands: Commands,
    mut scene_load: EventWriter<LoadJsSceneEvent>,
    user_script_folder: Res<UserScriptFolder>,
) {
    // add a camera
    commands.spawn(Camera3dBundle {
        transform: Transform::from_translation(Vec3::new(-10.0, 5.0, -4.0))
            .looking_at(Vec3::new(1.0, 3.0, 1.0), Vec3::Y),
        ..Default::default()
    });

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
    scene_load.send(LoadJsSceneEvent {
        scene: SceneDefinition {
            path: user_script_folder.0.clone(),
        },
    });
}
