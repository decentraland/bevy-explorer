use bevy::log::{error, info, warn};
use common::rpc::{IpcMessage, ResponseContext, ENGINE_IPC_CONTEXT};
use dcl::{
    interface::{CrdtComponentInterfaces, CrdtStore},
    RendererResponse, SceneId, SceneResponse,
};
use interprocess::local_socket::{
    tokio::{RecvHalf, SendHalf},
    traits::tokio::{Listener, Stream as _},
    GenericFilePath, ListenerOptions, ToFsName,
};
use ipfs::SceneJsFile;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    process::{Command, Stdio},
    sync::RwLock,
};
use system_bridge::SystemApi;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::mpsc::UnboundedSender,
};

#[derive(Serialize, Deserialize)]
pub struct NewSceneInfo {
    pub initial_crdt_store: CrdtStore,
    pub scene_hash: String,
    pub scene_js: String,
    pub crdt_component_interfaces: CrdtComponentInterfaces,
    pub id: SceneId,
    pub storage_root: String,
    pub inspect: bool,
    pub testing: bool,
    pub preview: bool,
    pub is_super: bool,
}

#[derive(Serialize, Deserialize)]
pub enum EngineToScene {
    NewScene(u64, NewSceneInfo),
    SceneUpdate(u64, RendererResponse),
    GlobalUpdate(Vec<u8>),
    IpcMessage(u64, IpcMessage),
}

#[derive(Serialize, Deserialize)]
pub enum SceneToEngine {
    SceneResponse(SceneResponse),
    SystemApi(SystemApi),
    IpcMessage(u64, IpcMessage),
}

thread_local! {
    static RENDERER_SENDER: RefCell<Option<tokio::sync::mpsc::UnboundedSender<SceneResponse>>> = const { RefCell::new(None) };
    static SYSTEM_API_SENDER: RefCell<Option<tokio::sync::mpsc::UnboundedSender<SystemApi>>> = const { RefCell::new(None) };
}

#[allow(clippy::type_complexity)]
pub static NEW_SCENE_SENDER: Lazy<
    RwLock<
        Option<
            tokio::sync::mpsc::UnboundedSender<(
                u64,
                NewSceneInfo,
                tokio::sync::mpsc::Receiver<RendererResponse>,
                tokio::sync::broadcast::Receiver<Vec<u8>>,
            )>,
        >,
    >,
> = Lazy::new(Default::default);

pub fn init_runtime() -> anyhow::Result<()> {
    let name_str = if cfg!(windows) {
        "bevy_explorer_ipc"
    } else {
        "/tmp/bevy_explorer_ipc.sock"
    };
    let name = name_str.to_fs_name::<GenericFilePath>()?;

    // 2. Bind the Listener
    let listener = ListenerOptions::new().name(name).create_tokio()?;

    // 3. Spawn Worker
    let mut _child = Command::new("target/release/dcl_deno_ipc")
        .arg(name_str)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;

    info!("[Host] Waiting for worker connection...");

    let (init_sx, init_rx) = tokio::sync::oneshot::channel::<anyhow::Result<()>>();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        // 4. Accept Connection
        info!("waiting for scene runtime initialization");
        let stream = match rt.block_on(async { listener.accept().await }) {
            Ok(stream) => stream,
            Err(e) => {
                error!("runtime initialization failed: {e}");
                let _ = init_sx.send(Err(e.into()));
                return;
            }
        };
        info!("scene runtime initialized");

        let (ipc_inbound, ipc_outbound) = stream.split();

        let (new_scene_sx, new_scene_rx) = tokio::sync::mpsc::unbounded_channel();
        let (router_sx, router_rx) = tokio::sync::mpsc::unbounded_channel();

        *NEW_SCENE_SENDER.write().unwrap() = Some(new_scene_sx);
        ENGINE_IPC_CONTEXT.set(Some(ResponseContext {
            ipc_channel_registry: Default::default(),
            ipc_router: router_sx,
        }));

        let f_out = rt.spawn(renderer_ipc_out(ipc_outbound, new_scene_rx, router_rx));
        let f_in = rt.spawn(renderer_ipc_in(ipc_inbound));

        let _ = init_sx.send(Ok(()));

        let _ = rt.block_on(async move { tokio::join!(f_out, f_in) });
    });

    init_rx.blocking_recv()?
}

#[allow(clippy::type_complexity)]
pub async fn renderer_ipc_out(
    mut stream: SendHalf,
    mut new_scene: tokio::sync::mpsc::UnboundedReceiver<(
        u64,
        NewSceneInfo,
        tokio::sync::mpsc::Receiver<RendererResponse>,
        tokio::sync::broadcast::Receiver<Vec<u8>>,
    )>,
    mut ipc_router: tokio::sync::mpsc::UnboundedReceiver<(u64, IpcMessage)>,
) {
    let (renderer_sx, mut renderer_rx) = tokio::sync::mpsc::unbounded_channel();

    let (_dummy_global_sx, mut global_rx) = tokio::sync::broadcast::channel(0);

    loop {
        tokio::select! {
            new_scene = new_scene.recv() => {
                let Some((id, info, mut channel, new_global_rx)) = new_scene else {
                    warn!("renderer_ipc_out exit on new scene closed");
                    return;
                };

                // might cause a couple of duplicated global messages for old scenes
                global_rx = new_global_rx;

                // spawn connector
                let renderer_sender = renderer_sx.clone();
                tokio::spawn(async move {
                    while let Some(renderer_response) = channel.recv().await {
                        renderer_sender.send((id, renderer_response)).unwrap();
                    }
                });

                write_msg(&mut stream, &EngineToScene::NewScene(id, info)).await;
            }
            renderer_update = renderer_rx.recv() => {
                let Some((id, response)) = renderer_update else {
                    warn!("renderer_ipc_out exit on inbound closed");
                    return;
                };
                write_msg(&mut stream, &EngineToScene::SceneUpdate(id, response)).await;
            }
            global_rx = global_rx.recv() => {
                let Ok(data) = global_rx else {
                    warn!("renderer_ipc_out exit on global receiver closed");
                    return;
                };
                write_msg(&mut stream, &EngineToScene::GlobalUpdate(data)).await;
            }
            ipc = ipc_router.recv() => {
                let Some(ipc) = ipc else {
                    warn!("renderer_ipc_out exit on router closed");
                    return;
                };
                write_msg(&mut stream, &EngineToScene::IpcMessage(ipc.0, ipc.1)).await;
            }
        }
    }
}

pub async fn renderer_ipc_in(mut stream: RecvHalf) {
    while let Ok(len) = stream.read_u64_le().await {
        let mut buffer = vec![0u8; len as usize];
        stream.read_exact(&mut buffer).await.unwrap();
        let msg: SceneToEngine = bincode::deserialize(&buffer).unwrap();

        match msg {
            SceneToEngine::SceneResponse(scene_response) => RENDERER_SENDER.with(|sender| {
                let mut sender = sender.borrow_mut();
                let sender = sender.as_mut().unwrap();
                sender.send(scene_response).unwrap();
            }),
            SceneToEngine::IpcMessage(id, ipc_message) => {
                let IpcMessage::Closed = ipc_message else {
                    panic!()
                };

                ENGINE_IPC_CONTEXT.with(|ctx| {
                    let mut ctx = ctx.borrow_mut();
                    let ctx = ctx.as_mut().unwrap();

                    if let Some(token) = ctx.ipc_channel_registry.remove(&id) {
                        token.cancel();
                    }
                })
            }
            SceneToEngine::SystemApi(system_command) => {
                SYSTEM_API_SENDER.with(|sender| {
                    let mut sender = sender.borrow_mut();
                    let sender = sender.as_mut().unwrap();

                    let _ = sender.send(system_command);
                });
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_scene(
    initial_crdt_store: CrdtStore,
    scene_hash: String,
    scene_js: SceneJsFile,
    crdt_component_interfaces: CrdtComponentInterfaces,
    renderer_sender: UnboundedSender<SceneResponse>,
    global_update_receiver: tokio::sync::broadcast::Receiver<Vec<u8>>,
    id: SceneId,
    storage_root: String,
    inspect: bool,
    testing: bool,
    preview: bool,
    super_user: Option<tokio::sync::mpsc::UnboundedSender<SystemApi>>,
) -> tokio::sync::mpsc::Sender<RendererResponse> {
    RENDERER_SENDER.set(Some(renderer_sender));

    let is_super = super_user.is_some();

    if let Some(super_user) = super_user {
        SYSTEM_API_SENDER.set(Some(super_user));
    }

    let (main_sx, thread_rx) = tokio::sync::mpsc::channel::<RendererResponse>(1);

    let ipc_out = NEW_SCENE_SENDER.read().unwrap();
    let ipc_out = ipc_out.as_ref().unwrap();

    ipc_out
        .send((
            id.0.to_bits(),
            NewSceneInfo {
                initial_crdt_store,
                scene_hash,
                scene_js: scene_js.0.to_string(),
                crdt_component_interfaces,
                id,
                storage_root,
                inspect,
                testing,
                preview,
                is_super,
            },
            thread_rx,
            global_update_receiver,
        ))
        .unwrap();

    main_sx
}

pub async fn write_msg<T: Serialize>(stream: &mut SendHalf, value: &T) {
    let bytes = bincode::serialize(value).unwrap();
    stream
        .write_all(&(bytes.len() as u64).to_le_bytes())
        .await
        .unwrap();
    stream.write_all(&bytes).await.unwrap();
}
