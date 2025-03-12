pub mod bake_scene;
pub mod floor_imposter;
pub mod imposter_spec;
pub mod render;

use std::path::PathBuf;

use bake_scene::DclImposterBakeScenePlugin;
use bevy::prelude::*;
use bevy_console::ConsoleCommand;
use common::structs::{AppConfig, SceneLoadDistance};
use console::DoAddConsoleCommand;
use render::{DclImposterRenderPlugin, ImposterEntities, SceneImposter};

#[derive(Resource, Clone)]
pub struct DclImposterPlugin {
    pub zip_output: Option<PathBuf>,
    pub download: bool,
}

impl Plugin for DclImposterPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((DclImposterBakeScenePlugin, DclImposterRenderPlugin))
            .add_console_command::<ImpostDistanceCommand, _>(set_impost_distance)
            .add_console_command::<ImpostMultisampleCommand, _>(set_impost_multi);
        app.insert_resource(self.clone());
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
    mut scene_distance: ResMut<SceneLoadDistance>,
) {
    if let Some(Ok(command)) = input.take() {
        let distances = command
            .distances
            .split(|c: char| !c.is_numeric() && c != '.')
            .map(str::parse::<f32>)
            .flat_map(Result::ok)
            .collect::<Vec<_>>();
        input.reply_ok(format!("imposter distances set to {distances:?}"));
        scene_distance.load_imposter = distances
            .last()
            .map(|last| {
                // actual distance we need is last + diagonal of the largest mip size
                let mip_size = (1 << (distances.len() - 1)) as f32 * 16.0;
                last + (2.0 * mip_size * mip_size).sqrt()
            })
            .unwrap_or(0.0);
        config.scene_imposter_distances = distances;
    }
}

#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/impost_multisample")]
struct ImpostMultisampleCommand {
    on: Option<u32>,
}

fn set_impost_multi(
    mut input: ConsoleCommand<ImpostMultisampleCommand>,
    mut config: ResMut<AppConfig>,
    mut commands: Commands,
    q: Query<Entity, With<SceneImposter>>,
    mut lookup: ResMut<ImposterEntities>,
) {
    if let Some(Ok(command)) = input.take() {
        let multisample = command
            .on
            .unwrap_or_else(|| {
                if config.scene_imposter_multisample {
                    0
                } else {
                    99
                }
            })
            .clamp(0, 99) as f32
            / 100.0;
        config.scene_imposter_multisample = multisample != 0.0;
        config.scene_imposter_multisample_amount = multisample;
        input.reply_ok("imposter multisample set to {multisample}");
        for e in q.iter() {
            commands.entity(e).despawn_recursive();
        }

        lookup.0.retain(|(_, _, ingredient), _| *ingredient);
    }
}
