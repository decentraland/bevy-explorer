use std::path::PathBuf;

use bevy::{
    asset::AssetIo,
    prelude::*,
    tasks::{IoTaskPool, Task},
};
use bevy_console::ConsoleCommand;
use common::{structs::PrimaryUser, util::TaskExt};
use console::DoAddConsoleCommand;
use ipfs::{
    ipfs_path::{IpfsPath, IpfsType},
    EntityDefinition, IpfsLoaderExt,
};

use crate::{renderer_context::RendererSceneContext, ContainingScene};

pub struct SceneUtilPlugin;

impl Plugin for SceneUtilPlugin {
    fn build(&self, app: &mut App) {
        app.add_console_command::<DebugDumpScene, _>(debug_dump_scene);
    }
}

#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/debug_dump_scene")]
struct DebugDumpScene;

#[allow(clippy::too_many_arguments)]
fn debug_dump_scene(
    mut input: ConsoleCommand<DebugDumpScene>,
    containing_scene: ContainingScene,
    player: Query<Entity, With<PrimaryUser>>,
    scene: Query<&RendererSceneContext>,
    asset_server: Res<AssetServer>,
    scene_definitions: Res<Assets<EntityDefinition>>,
    mut tasks: Local<Vec<Task<Option<String>>>>,
    mut response: Local<Vec<Option<String>>>,
) {
    if let Some(Ok(_)) = input.take() {
        let Some(scene) = player
            .get_single()
            .ok()
            .and_then(|p| containing_scene.get(p))
            .and_then(|s| scene.get(s).ok())
        else {
            input.reply_failed("no scene");
            return;
        };

        let h_scene = asset_server.load_hash::<EntityDefinition>(&scene.hash);
        let Some(def) = scene_definitions.get(&h_scene) else {
            input.reply_failed("can't resolve scene handle");
            return;
        };

        let dump_folder = asset_server
            .ipfs()
            .cache_path()
            .to_owned()
            .join("scene_dump")
            .join(&scene.hash);
        std::fs::create_dir_all(&dump_folder).unwrap();

        for content_file in def.content.files() {
            let ipfs_path = IpfsPath::new(IpfsType::new_content_file(
                scene.hash.to_owned(),
                content_file.to_owned(),
            ));

            let path = PathBuf::from(&ipfs_path);

            let asset_server = asset_server.clone();
            let content_file = content_file.clone();
            let dump_folder = dump_folder.clone();
            tasks.push(IoTaskPool::get().spawn(async move {
                let Ok(bytes) = asset_server.ipfs().load_path(&path).await else {
                    return Some(format!("{content_file} failed: couldn't load bytes\n"));
                };

                let file = dump_folder.join(&content_file);
                if let Some(parent) = file.parent() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        return Some(format!(
                            "{content_file} failed: couldn't create parent: {e}\n"
                        ));
                    }
                }
                if let Err(e) = std::fs::write(file, bytes) {
                    return Some(format!("{content_file} failed: {e}\n"));
                }

                None
            }));
        }
    }

    if tasks.len() > 0 {
        tasks.retain_mut(|t| match t.complete() {
            None => true,
            Some(resp) => {
                response.push(resp);
                false
            }
        });
        if tasks.is_empty() {
            let tasks = response.len();
            let errs = response.iter().flatten().collect::<Vec<_>>();
            if errs.is_empty() {
                input.reply_ok("All good");
            } else {
                input.reply_failed(format!(
                    "{}/{} files saved successfully. Errors:",
                    tasks - errs.len(),
                    tasks
                ));
                for err in errs {
                    input.reply_failed(err);
                }
            }

            response.clear();
        }
    }
}
