use anyhow::Result;
use bevy::{log::warn, platform::collections::HashMap};
use common::rpc::{IpcMessage, RequestContext, SCENE_IPC_CONTEXT};
use dcl::SceneResponse;
use dcl_deno_ipc::{write_msg, EngineToScene, SceneToEngine};
use interprocess::local_socket::{
    tokio::{RecvHalf, SendHalf, Stream},
    traits::tokio::Stream as _,
    GenericFilePath, ToFsName,
};
use system_bridge::SystemApi;
use std::{env, sync::Arc};
use tokio::io::AsyncReadExt;

fn main() -> Result<()> {
    // spawn runtime
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    // get the socket name from args
    let name_str = env::args().nth(1).expect("No socket name provided");
    let name = name_str.to_fs_name::<GenericFilePath>()?;

    // connect ipc stream
    let stream = rt.block_on(async move { Stream::connect(name).await.unwrap() });

    let (recv_half, send_half) = stream.split();

    // init engine
    dcl_deno::init_runtime();

    // init context
    let (close_sx, close_rx) = tokio::sync::mpsc::unbounded_channel();
    let (scene_sx, scene_rx) = tokio::sync::mpsc::unbounded_channel();
    let (system_api_sx, system_api_rx) = tokio::sync::mpsc::unbounded_channel();
    SCENE_IPC_CONTEXT.set(Some(RequestContext {
        registry: Default::default(),
        close_sender: close_sx,
        next_id: 1,
    }));

    let f_in = rt.spawn(scene_ipc_in(recv_half, scene_sx, system_api_sx));
    let f_out = rt.spawn(scene_ipc_out(send_half, scene_rx, close_rx, system_api_rx));

    let _ = rt.block_on(async move { tokio::join!(f_in, f_out) });

    Ok(())
}

async fn scene_ipc_out(
    mut stream: SendHalf,
    mut scene_rx: tokio::sync::mpsc::UnboundedReceiver<SceneResponse>,
    mut close_rx: tokio::sync::mpsc::UnboundedReceiver<u64>,
    mut system_api_rx: tokio::sync::mpsc::UnboundedReceiver<SystemApi>,
) {
    tokio::select! {
        scene_rx = scene_rx.recv() => {
            let Some(scene_rx) = scene_rx else {
                warn!("scene_ipc_out exit on scene_rx closed");
                return;
            };
            write_msg(&mut stream, &SceneToEngine::SceneResponse(scene_rx)).await;
        }
        close_rx = close_rx.recv() => {
            let Some(close_id) = close_rx else {
                warn!("scene_ipc_out exit on close_rx closed");
                return;
            };

            let was_open = SCENE_IPC_CONTEXT.with(|ctx| {
                let mut ctx = ctx.borrow_mut();
                let ctx = ctx.as_mut().unwrap();
                ctx.registry.remove(&close_id).is_some()
            });

            if was_open {
                write_msg(&mut stream, &SceneToEngine::IpcMessage(close_id, IpcMessage::Closed)).await;                
            }
        },
        system_api_rx = system_api_rx.recv() => {
            let Some(system_api) = system_api_rx else {
                warn!("scene_ipc_out exit on system_api_rx closed");
                return;
            };
            write_msg(&mut stream, &SceneToEngine::SystemApi(system_api)).await;
        }
    }
}

async fn scene_ipc_in(
    mut stream: RecvHalf,
    scene_sx: tokio::sync::mpsc::UnboundedSender<SceneResponse>,
    system_api_sx: tokio::sync::mpsc::UnboundedSender<SystemApi>,
) {
    let mut renderer_senders = HashMap::new();

    let (global_sx, _global_rx) = tokio::sync::broadcast::channel(1000);

    while let Ok(len) = stream.read_u64_le().await {
        let mut buffer = vec![0u8; len as usize];
        stream.read_exact(&mut buffer).await.unwrap();
        let msg: EngineToScene = bincode::deserialize(&buffer).unwrap();

        match msg {
            EngineToScene::NewScene(id, new_scene_info) => {
                let response_sx = dcl_deno::spawn_scene(
                    new_scene_info.initial_crdt_store,
                    new_scene_info.scene_hash,
                    ipfs::SceneJsFile(Arc::new(new_scene_info.scene_js)),
                    new_scene_info.crdt_component_interfaces,
                    scene_sx.clone(),
                    global_sx.subscribe(),
                    new_scene_info.id,
                    new_scene_info.storage_root,
                    new_scene_info.inspect,
                    new_scene_info.testing,
                    new_scene_info.preview,
                    new_scene_info.is_super.then(|| system_api_sx.clone()),
                );

                renderer_senders.insert(id, response_sx);
            }
            EngineToScene::SceneUpdate(id, renderer_response) => {
                let Some(sender) = renderer_senders.get(&id) else {
                    warn!("no sender with id {id}");
                    continue;
                };

                let _ = sender.send(renderer_response).await;
            }
            EngineToScene::GlobalUpdate(data) => {
                let _ = global_sx.send(data);
            }
            EngineToScene::IpcMessage(id, ipc_message) => {
                SCENE_IPC_CONTEXT.with(|ctx| {
                    let mut ctx = ctx.borrow_mut();
                    let ctx = ctx.as_mut().unwrap();

                    match ipc_message {
                        common::rpc::IpcMessage::Data(data) => {
                            if let Some(endpoint) = ctx.registry.get_mut(&id) {
                                endpoint.send(data);
                            }
                        }
                        common::rpc::IpcMessage::Closed => {
                            let _ = ctx.registry.remove(&id);
                        }
                    }
                });
            }
        }
    }
}
