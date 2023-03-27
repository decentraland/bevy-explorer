/*

use bevy::prelude::*;
use deno_core::serde::Serialize;

use crate::scene_runner::{EngineResponse, SceneSets};

// plugin to pass user input messages to the scene
pub struct SceneInputPlugin;

impl Plugin for SceneInputPlugin {
    fn build(&self, app: &mut App) {
        // register system
        // app.add_system(send_key_input.in_set(SceneSets::Input));
    }
}


// any struct implementing Serialize can be fed to EngineResponse::new()
#[derive(Serialize)]
struct EngineKeyPressed {
    key: String,
}

fn send_key_input(mut writer: EventWriter<EngineResponse>, input: Res<Input<KeyCode>>) {
    for &key in input.get_just_pressed() {
        writer.send(EngineResponse::new(
            "key_down".to_owned(),
            EngineKeyPressed {
                key: format!("{key:?}").to_lowercase(),
            },
        ));
    }

    for &key in input.get_just_released() {
        writer.send(EngineResponse::new(
            "key_up".to_owned(),
            EngineKeyPressed {
                key: format!("{key:?}").to_lowercase(),
            },
        ));
    }
}

*/
