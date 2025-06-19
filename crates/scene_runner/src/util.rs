use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use base64::{prelude::BASE64_URL_SAFE_NO_PAD, Engine};
use bevy::{
    asset::io::AssetReader,
    prelude::*,
    tasks::{IoTaskPool, Task},
};
use bevy_console::{ConsoleCommand, PrintConsoleLine};
use common::{
    structs::{PreviewCommand, PrimaryUser},
    util::TaskExt,
};
use console::DoAddConsoleCommand;
use futures_lite::AsyncReadExt;
use ipfs::{
    ipfs_path::{IpfsPath, IpfsType},
    EntityDefinition, IpfsAssetServer,
};
use multihash_codetable::MultihashDigest;

use crate::{
    initialize_scene::LiveScenes, renderer_context::RendererSceneContext, ContainingScene, Toaster,
};

pub struct SceneUtilPlugin;

impl Plugin for SceneUtilPlugin {
    fn build(&self, app: &mut App) {
        let (send, recv) = tokio::sync::mpsc::unbounded_channel();
        app.insert_resource(ConsoleRelay { send, recv });
        app.add_console_command::<DebugDumpScene, _>(debug_dump_scene);
        app.add_console_command::<ReloadCommand, _>(reload_command);
        app.add_console_command::<ClearStoreCommand, _>(clear_store_command);
        app.add_systems(Update, (console_relay, handle_preview_command));
    }
}

#[derive(Resource)]
pub struct ConsoleRelay {
    pub send: tokio::sync::mpsc::UnboundedSender<String>,
    recv: tokio::sync::mpsc::UnboundedReceiver<String>,
}

fn console_relay(mut write: EventWriter<PrintConsoleLine>, mut relay: ResMut<ConsoleRelay>) {
    while let Ok(line) = relay.recv.try_recv() {
        write.write(PrintConsoleLine { line });
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
    ipfas: IpfsAssetServer,
    scene_definitions: Res<Assets<EntityDefinition>>,
    mut tasks: Local<Vec<Task<()>>>,
    console_relay: Res<ConsoleRelay>,
) {
    if let Some(Ok(_)) = input.take() {
        let scenes = player
            .single()
            .ok()
            .map(|p| containing_scene.get(p))
            .unwrap_or_default()
            .into_iter()
            .flat_map(|s| scene.get(s).ok())
            .collect::<Vec<_>>();

        if scenes.is_empty() {
            input.reply_failed("no scenes");
            return;
        };

        for scene in scenes {
            let h_scene = ipfas.load_hash::<EntityDefinition>(&scene.hash);
            let Some(def) = scene_definitions.get(&h_scene) else {
                input.reply_failed("can't resolve scene handle");
                return;
            };

            if ipfas.ipfs().cache_path().is_none() {
                warn!("no cache");
                return;
            }

            let dump_folder = ipfas
                .ipfs()
                .cache_path()
                .unwrap()
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

                let ipfs = ipfas.ipfs().clone();
                let content_file = content_file.clone();
                let dump_folder = dump_folder.clone();
                let count = count.clone();
                let send = console_relay.send.clone();
                tasks.push(IoTaskPool::get().spawn(async move {
                    let report = |fail: Option<String>| {
                        let mut count = count.lock().unwrap();
                        if let Some(fail) = fail {
                            count.2 += 1;
                            let _ = send.send(fail);
                        } else {
                            count.1 += 1;
                        }
                        if count.0 == count.1 + count.2 {
                            if count.2 == 0 {
                                let _ = send.send(format!("[ok] {} files downloaded", count.0));
                            } else {
                                let _ = send.send(format!(
                                    "[failed] {}/{} files downloaded",
                                    count.1, count.0
                                ));
                            }
                        }
                    };

                    let Ok(mut reader) = ipfs.read(&path).await else {
                        report(Some(format!(
                            "{content_file} failed: couldn't load bytes\n"
                        )));
                        return;
                    };
                    let mut bytes = Vec::default();
                    if let Err(e) = reader.read_to_end(&mut bytes).await {
                        report(Some(format!("{content_file} failed: {e}")));
                        return;
                    }

                    let file = dump_folder.join(&content_file);
                    if let Some(parent) = file.parent() {
                        if let Err(e) = std::fs::create_dir_all(parent) {
                            report(Some(format!(
                                "{content_file} failed: couldn't create parent: {e}"
                            )));
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

            input.reply(format!(
                "scene hash {}, downloading {} files",
                scene.hash,
                tasks.len()
            ));
        }
    }

    tasks.retain_mut(|t| t.complete().is_none());
}

#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/reload")]
struct ReloadCommand {
    hash: Option<String>,
}

fn reload_command(mut input: ConsoleCommand<ReloadCommand>, mut live_scenes: ResMut<LiveScenes>) {
    if let Some(Ok(ReloadCommand { hash })) = input.take() {
        match hash {
            Some(hash) => {
                if live_scenes.scenes.remove(&hash).is_some() {
                    input.reply_ok(format!("{hash} reloaded"));
                } else {
                    input.reply_failed(format!("{hash} not found"));
                }
            }
            None => {
                live_scenes.scenes.clear();
                input.reply_ok("all scenes reloaded");
            }
        }
    }
}

#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/clear_store")]
struct ClearStoreCommand {
    hash: Option<String>,
}

fn clear_store_command(
    mut input: ConsoleCommand<ClearStoreCommand>,
    mut live_scenes: ResMut<LiveScenes>,
    contexts: Query<&RendererSceneContext>,
    mut clear_task: Local<Option<Task<()>>>,
) {
    if let Some(mut task) = clear_task.take() {
        if task.complete().is_some() {
            live_scenes.block_new_scenes = false;
        } else {
            *clear_task = Some(task);
            return;
        }
    }

    let mut remove = |parent: &Path, file: &str| {
        let storage_folder = parent.join(file);

        if std::fs::exists(&storage_folder).unwrap_or_default() {
            let temp = parent.join(format!("{file}_delete"));
            *clear_task = Some(IoTaskPool::get().spawn(async move {
                while let Err(e) = std::fs::rename(&storage_folder, &temp) {
                    warn!("can't rename {temp:?}: {e}");
                    async_std::task::sleep(Duration::from_millis(500)).await;
                }

                while let Err(e) = std::fs::remove_dir_all(&temp) {
                    warn!("can't delete {temp:?}: {e}");
                    async_std::task::sleep(Duration::from_millis(500)).await;
                }
            }));
        }
    };

    if let Some(Ok(ClearStoreCommand { hash })) = input.take() {
        let Some(project_directories) = platform::project_directories() else {
            warn!("not implemented");
            return;
        };

        if let Some(hash) = hash {
            if let Some(ctx) = live_scenes
                .scenes
                .remove(&hash)
                .and_then(|e| contexts.get(e).ok())
            {
                let storage_digest =
                    multihash_codetable::Code::Sha2_256.digest(ctx.storage_root.as_bytes());
                let storage_hash = BASE64_URL_SAFE_NO_PAD.encode(storage_digest.digest());
                remove(
                    &project_directories.data_local_dir().join("LocalStorage"),
                    &storage_hash,
                );
            }
            live_scenes.block_new_scenes = clear_task.is_some();
        } else {
            // all
            live_scenes.scenes.clear();
            remove(project_directories.data_local_dir(), "LocalStorage");
            live_scenes.block_new_scenes = clear_task.is_some();
        }
    }
}

fn handle_preview_command(
    mut events: EventReader<PreviewCommand>,
    mut live_scenes: ResMut<LiveScenes>,
    scenes: Query<&RendererSceneContext>,
    mut toaster: Toaster,
) {
    for command in events.read() {
        match command {
            PreviewCommand::ReloadScene { hash } => {
                if let Some(ctx) = live_scenes
                    .scenes
                    .get(hash)
                    .and_then(|e| scenes.get(*e).ok())
                {
                    if ctx.inspected {
                        toaster.add_toast("reload-inspected", "Scene has updated but an inspector is attached. To force the reload type \"/reload\" in the chat window");
                        continue;
                    }
                };
                live_scenes.scenes.remove(hash);
            }
        }
    }
}
