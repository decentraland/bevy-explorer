pub mod bake_scene;
pub mod floor_imposter;
pub mod imposter_mesh;
pub mod imposter_spec;
pub mod render;

use std::path::PathBuf;

use bake_scene::DclImposterBakeScenePlugin;
use bevy::{
    prelude::*,
    tasks::{IoTaskPool, Task},
};
use bevy_console::ConsoleCommand;
use common::{
    structs::{AppConfig, SceneLoadDistance},
    util::{TaskCompat, TaskExt},
};
use console::DoAddConsoleCommand;
use ipfs::CurrentRealm;
use render::{DclImposterRenderPlugin, SceneImposter};
use reqwest::{Client, StatusCode};

#[derive(Resource, Clone)]
pub struct DclImposterPlugin {
    pub zip_output: Option<PathBuf>,
    pub download: bool,
}

impl Plugin for DclImposterPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<ImpostersAllowed>();

        app.add_plugins((DclImposterBakeScenePlugin, DclImposterRenderPlugin))
            .add_console_command::<ImpostDistanceCommand, _>(set_impost_distance)
            .add_console_command::<ImpostMultisampleCommand, _>(set_impost_multi);
        app.insert_resource(self.clone());

        app.add_systems(
            Update,
            (
                realm_changed.run_if(resource_exists_and_changed::<CurrentRealm>),
                verify_cors.run_if(resource_exists::<TestingCors>),
            ),
        );
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, States)]
enum ImpostersAllowed {
    #[default]
    Disallowed,
    Allowed,
}

#[derive(Resource)]
struct TestingCors {
    task: Task<Result<StatusCode, reqwest::Error>>,
}

/// SAFETY: should be fine while WASM remains single-threaded
#[cfg(target_arch = "wasm32")]
unsafe impl Send for TestingCors {}

/// SAFETY: should be fine while WASM remains single-threaded
#[cfg(target_arch = "wasm32")]
unsafe impl Sync for TestingCors {}

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
            commands.entity(e).despawn();
        }
    }
}

fn realm_changed(mut commands: Commands, current_realm: Res<CurrentRealm>) {
    commands.set_state(ImpostersAllowed::Disallowed);
    if let Some(ref realm_name) = current_realm.config.realm_name {
        debug!("Realm changed to {:?}", realm_name);
        if realm_name == "main" {
            let task_pool = IoTaskPool::get();
            let task = task_pool.spawn_compat(async {
                let client = Client::new();
                let request = client
                    .head("https://imposter.kuruk.net/v1/scenes.json")
                    .build()?;
                client
                    .execute(request)
                    .await
                    .map(|response| response.status())
            });
            commands.insert_resource(TestingCors { task });
        }
    }
}

fn verify_cors(mut commands: Commands, mut testing_cors: ResMut<TestingCors>) {
    if let Some(maybe_status_code) = testing_cors.task.complete() {
        match maybe_status_code {
            Ok(status_code) => {
                if status_code.is_success() {
                    commands.set_state(ImpostersAllowed::Allowed);
                }
            }
            Err(err) => {
                error!("{err}");
            }
        }
        commands.remove_resource::<TestingCors>();
    }
}
