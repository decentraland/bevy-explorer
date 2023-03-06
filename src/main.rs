mod input_handler;
mod output_handler;
mod scene_runner;
mod test;

use bevy::prelude::*;

use input_handler::SceneInputPlugin;
use output_handler::SceneOutputPlugin;
use scene_runner::{JsScene, LoadJsSceneEvent, SceneRunnerPlugin};

#[derive(Resource)]
struct UserScriptFolder(String);

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let user_script_folder = args.get(1).expect("please enter script folder");

    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugin(SceneRunnerPlugin) // script engine plugin
        .add_plugin(SceneInputPlugin) // plugin for posting input events to the script
        .add_plugin(SceneOutputPlugin) // plugin for processing some commands from the script
        .add_startup_system(setup)
        .insert_resource(UserScriptFolder(user_script_folder.clone()))
        .run()
}

fn setup(
    mut commands: Commands,
    mut scene_load: EventWriter<LoadJsSceneEvent>,
    user_script_folder: Res<UserScriptFolder>,
) {
    // add a camera
    commands.spawn(Camera3dBundle {
        transform: Transform::from_translation(Vec3::new(1.0, 1.0, 3.0))
            .looking_at(Vec3::ZERO, Vec3::Y),
        ..Default::default()
    });

    // load the scene
    scene_load.send(LoadJsSceneEvent {
        scene: JsScene {
            path: user_script_folder.0.clone(),
        },
    });
}
