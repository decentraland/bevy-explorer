use std::{path::PathBuf, sync::{Arc, Mutex}};

use bevy::{
    asset::AssetIo,
    prelude::*,
    tasks::{IoTaskPool, Task},
};
use bevy_console::{ConsoleCommand, PrintConsoleLine};
use clap::builder::StyledStr;
use common::structs::PrimaryUser;
use console::DoAddConsoleCommand;
use ipfs::{
    ipfs_path::{IpfsPath, IpfsType},
    EntityDefinition, IpfsLoaderExt,
};

use crate::{renderer_context::RendererSceneContext, ContainingScene};

pub struct SceneUtilPlugin;

impl Plugin for SceneUtilPlugin {
    fn build(&self, app: &mut App) {
        let (send, recv) = tokio::sync::mpsc::unbounded_channel();
        app.insert_resource(ConsoleRelay{ send, recv });
        app.add_console_command::<DebugDumpScene, _>(debug_dump_scene);
        app.add_systems(Update, console_relay);
    }
}

#[derive(Resource)]
pub struct ConsoleRelay {
    pub send: tokio::sync::mpsc::UnboundedSender<StyledStr>,
    recv: tokio::sync::mpsc::UnboundedReceiver<StyledStr>,
}

fn console_relay(
    mut write: EventWriter<PrintConsoleLine>,
    mut relay: ResMut<ConsoleRelay>,
) {
    while let Ok(line) = relay.recv.try_recv() {
        write.send(PrintConsoleLine { line });
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
    mut tasks: Local<Vec<Task<()>>>,
    console_relay: Res<ConsoleRelay>,
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

        // total / succeed / fail
        let count = Arc::new(Mutex::new((0, 0, 0)));

        for content_file in def.content.files() {
            count.lock().unwrap().0 += 1;
            let ipfs_path = IpfsPath::new(IpfsType::new_content_file(
                scene.hash.to_owned(),
                content_file.to_owned(),
            ));

            let path = PathBuf::from(&ipfs_path);

            let asset_server = asset_server.clone();
            let content_file = content_file.clone();
            let dump_folder = dump_folder.clone();
            let count = count.clone();
            let send = console_relay.send.clone();
            tasks.push(IoTaskPool::get().spawn(async move {
                let report = |fail: Option<String>| {
                    let mut count = count.lock().unwrap();
                    if let Some(fail) = fail {
                        count.2 += 1;
                        let _ = send.send(fail.into());
                    } else {
                        count.1 += 1;
                    }
                    if count.0 == count.1 + count.2 {
                        if count.2 == 0 {
                            let _ = send.send(format!("[ok] {} files downloaded", count.0).into());
                        } else {
                            let _ = send.send(format!("[failed] {}/{} files downloaded", count.1, count.0).into());
                        }
                    }
                };

                let Ok(bytes) = asset_server.ipfs().load_path(&path).await else {
                    report(Some(format!("{content_file} failed: couldn't load bytes\n")));
                    return;
                };

                let file = dump_folder.join(&content_file);
                if let Some(parent) = file.parent() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        report(Some(format!("{content_file} failed: couldn't create parent: {e}")));
                        return;
                    }
                }
                if let Err(e) = std::fs::write(file, bytes) {
                    report(Some(format!("{content_file} failed: {e}")));
                    return;
                }

                report(None);
            }));
        }

        input.reply(format!("scene hash {}, downloading {} files", scene.hash, tasks.len()));
    }

    tasks.retain_mut(|t| !t.is_finished());
}
