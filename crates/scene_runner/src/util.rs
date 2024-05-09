use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use bevy::{
    asset::io::AssetReader,
    prelude::*,
    tasks::{IoTaskPool, Task},
};
use bevy_console::{ConsoleCommand, PrintConsoleLine};
use clap::builder::StyledStr;
use common::structs::PrimaryUser;
use comms::preview::PreviewCommand;
use console::DoAddConsoleCommand;
use futures_lite::AsyncReadExt;
use ipfs::{
    ipfs_path::{IpfsPath, IpfsType},
    EntityDefinition, IpfsAssetServer,
};

use crate::{
    initialize_scene::{LiveScenes, PortableScenes},
    renderer_context::RendererSceneContext,
    ContainingScene,
};

pub struct SceneUtilPlugin;

impl Plugin for SceneUtilPlugin {
    fn build(&self, app: &mut App) {
        let (send, recv) = tokio::sync::mpsc::unbounded_channel();
        app.insert_resource(ConsoleRelay { send, recv });
        app.add_console_command::<DebugDumpScene, _>(debug_dump_scene);
        app.add_console_command::<ReloadCommand, _>(reload_command);
        app.add_systems(Update, (console_relay, handle_preview_command));
    }
}

#[derive(Resource)]
pub struct ConsoleRelay {
    pub send: tokio::sync::mpsc::UnboundedSender<StyledStr>,
    recv: tokio::sync::mpsc::UnboundedReceiver<StyledStr>,
}

fn console_relay(mut write: EventWriter<PrintConsoleLine>, mut relay: ResMut<ConsoleRelay>) {
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
    ipfas: IpfsAssetServer,
    scene_definitions: Res<Assets<EntityDefinition>>,
    mut tasks: Local<Vec<Task<()>>>,
    console_relay: Res<ConsoleRelay>,
) {
    if let Some(Ok(_)) = input.take() {
        let scenes = player
            .get_single()
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

            let dump_folder = ipfas
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
                            let _ = send.send(fail.into());
                        } else {
                            count.1 += 1;
                        }
                        if count.0 == count.1 + count.2 {
                            if count.2 == 0 {
                                let _ =
                                    send.send(format!("[ok] {} files downloaded", count.0).into());
                            } else {
                                let _ = send.send(
                                    format!("[failed] {}/{} files downloaded", count.1, count.0)
                                        .into(),
                                );
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

    tasks.retain_mut(|t| !t.is_finished());
}

#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/reload")]
struct ReloadCommand {
    hash: Option<String>,
}

fn reload_command(
    mut input: ConsoleCommand<ReloadCommand>,
    mut live_scenes: ResMut<LiveScenes>,
    mut portables: ResMut<PortableScenes>,
) {
    if let Some(Ok(ReloadCommand { hash })) = input.take() {
        match hash {
            Some(hash) => {
                live_scenes.0.remove(&hash);
                portables.0.remove(&hash);
            }
            None => {
                live_scenes.0.clear();
                portables.0.clear();
            }
        }
    }
}

fn handle_preview_command(
    mut events: EventReader<PreviewCommand>,
    mut live_scenes: ResMut<LiveScenes>,
    mut portables: ResMut<PortableScenes>,
) {
    for command in events.read() {
        match command {
            PreviewCommand::ReloadScene { hash } => {
                live_scenes.0.remove(hash);
                portables.0.remove(hash);
            }
        }
    }
}
