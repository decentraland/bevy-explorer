pub mod bake_scene;
pub mod floor_imposter;
pub mod imposter_spec;
pub mod render;

use bake_scene::DclImposterBakeScenePlugin;
use bevy::prelude::*;
use bevy_console::ConsoleCommand;
use common::structs::AppConfig;
use console::DoAddConsoleCommand;
use render::{DclImposterRenderPlugin, SceneImposter};

pub struct DclImposterPlugin;

impl Plugin for DclImposterPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((DclImposterBakeScenePlugin, DclImposterRenderPlugin))
            .add_console_command::<ImpostDistanceCommand, _>(set_impost_distance)
            .add_console_command::<ImpostMultisampleCommand, _>(set_impost_multi);
    }
}

#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/impost")]
struct ImpostDistanceCommand {
    distances: String,
}

fn set_impost_distance(
    mut input: ConsoleCommand<ImpostDistanceCommand>,
    mut config: ResMut<AppConfig>,
) {
    if let Some(Ok(command)) = input.take() {
        let distances = command
            .distances
            .split(|c: char| !c.is_numeric() && c != '.')
            .map(str::parse::<f32>)
            .flat_map(Result::ok)
            .collect::<Vec<_>>();
        input.reply_ok(format!("imposter distances set to {distances:?}"));
        config.scene_imposter_distances = distances;
    }
}

#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/impost_multisample")]
struct ImpostMultisampleCommand {
    on: Option<bool>,
}

fn set_impost_multi(
    mut input: ConsoleCommand<ImpostMultisampleCommand>,
    mut config: ResMut<AppConfig>,
    mut commands: Commands,
    q: Query<Entity, With<SceneImposter>>,
) {
    if let Some(Ok(command)) = input.take() {
        let multisample = command.on.unwrap_or(!config.scene_imposter_multisample);
        config.scene_imposter_multisample = multisample;
        input.reply_ok("imposter multisample set to {multisample}");
        for e in q.iter() {
            commands.entity(e).despawn_recursive();
        }
    }
}
