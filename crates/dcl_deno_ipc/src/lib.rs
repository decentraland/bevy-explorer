use anyhow::anyhow;
use bevy::log::{debug, error, warn};
use common::{
    rpc::{rmp_encode, IpcMessage, ResponseContext, ENGINE_IPC_CONTEXT},
    structs::GlobalCrdtStateUpdate,
};
use dcl::{
    interface::{crdt_context::CrdtContext, CrdtComponentInterfaces, CrdtStore},
    js::SceneResponseSender,
    RendererResponse, SceneResponse,
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
    sync::{
        atomic::{AtomicBool, Ordering},
        RwLock,
    },
};
use system_bridge::SystemApi;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Serialize, Deserialize)]
pub struct NewSceneInfo {
    pub initial_crdt_store: CrdtStore,
    pub scene_context: CrdtContext,
    pub scene_js: String,
    pub crdt_component_interfaces: CrdtComponentInterfaces,
    pub storage_root: String,
    pub inspect: bool,
    pub is_super: bool,
    pub scene_origin: bevy::prelude::Vec3,
}

#[derive(Serialize, Deserialize)]
pub enum EngineToScene {
    NewScene(u64, Box<NewSceneInfo>),
    SceneUpdate(u64, RendererResponse),
    KillScene(u64),
    GlobalUpdate(GlobalCrdtStateUpdate),
    IpcMessage(u64, IpcMessage),
}

#[derive(Serialize, Deserialize)]
pub enum SceneToEngine {
    SceneResponse(SceneResponse),
    SystemApi(SystemApi),
    IpcMessage(u64, IpcMessage),
}

thread_local! {
    static RENDERER_SENDER: RefCell<Option<SceneResponseSender>> = const { RefCell::new(None) };
    static SYSTEM_API_SENDER: RefCell<Option<tokio::sync::mpsc::UnboundedSender<SystemApi>>> = const { RefCell::new(None) };
}

pub struct NewSceneCommand {
    id: u64,
    info: NewSceneInfo,
    renderer_channel: tokio::sync::mpsc::UnboundedReceiver<RendererResponse>,
    global_channel: tokio::sync::broadcast::Receiver<GlobalCrdtStateUpdate>,
    response_channel: SceneResponseSender,
    system_api_sender: Option<tokio::sync::mpsc::UnboundedSender<SystemApi>>,
}

#[allow(clippy::type_complexity)]
pub static NEW_SCENE_SENDER: Lazy<
    RwLock<Option<tokio::sync::mpsc::UnboundedSender<NewSceneCommand>>>,
> = Lazy::new(Default::default);

/// Opt-in for the orchestrated headless server ONLY: when set, a runtime whose IPC
/// loops end (the JS sidecar was lost) hard-exits the process so the supervisor can
/// restart the whole engine — otherwise a half-dead engine looks healthy forever and
/// the parent's engine-down recovery never fires. Left false for the desktop client
/// and for tests, which build a fresh runtime per app and must not kill the process.
pub static EXIT_ON_SIDECAR_LOSS: AtomicBool = AtomicBool::new(false);

pub fn init_runtime() -> anyhow::Result<()> {
    let (init_sx, init_rx) = tokio::sync::oneshot::channel::<anyhow::Result<()>>();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        let process_id = std::process::id();
        let random_id = fastrand::usize(0..0xffff);
        let name_str = if cfg!(windows) {
            format!(r"\\.\pipe\bevy_explorer_ipc_{process_id:x}_{random_id}")
        } else {
            let temp_dir = std::env::temp_dir();
            let socket_path =
                temp_dir.join(format!("bevy_explorer_{process_id:x}_{random_id}.sock"));
            socket_path.to_string_lossy().into_owned()
        };
        let name = name_str.clone().to_fs_name::<GenericFilePath>().unwrap();

        let listener = rt.block_on(async { ListenerOptions::new().name(name).create_tokio() });
        let listener = match listener {
            Ok(l) => l,
            Err(e) => {
                error!("failed to create listener: {e}");
                let _ = init_sx.send(Err(anyhow!(e)));
                return;
            }
        };

        let mut target = std::env::current_exe().unwrap();
        target.pop();

        // pop an extra folder when running tests
        if target.file_name().and_then(|s| s.to_str()) == Some("deps") {
            target.pop();
        }

        let mut target = target.join("dcl_deno_ipc");
        if cfg!(windows) {
            target.set_extension("exe");
        }

        let mut command = Command::new(&target);
        command
            .arg(name_str)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        // A console-subsystem child spawned by the GUI-subsystem app has no console to
        // inherit, so windows pops a visible one; suppress it. The inherited handles
        // still carry output to the parent's console when it has one (console builds).
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command
            .spawn()
            .unwrap_or_else(|_| panic!("failed to spawn deno binary at {target:?}"));

        let stream = match rt.block_on(async { listener.accept().await }) {
            Ok(stream) => stream,
            Err(e) => {
                error!("runtime initialization failed: {e}");
                let _ = init_sx.send(Err(e.into()));
                child.wait().unwrap();
                return;
            }
        };

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
        let _ = child.wait();

        // Orchestrated headless server only (EXIT_ON_SIDECAR_LOSS): the IPC loops ended,
        // so the JS sidecar is gone and the engine can run no scene code. Nothing here
        // can respawn it, and the parent's engine-down recovery never fires for a still-
        // running process — so exit hard and let the supervisor restart the whole engine.
        // Desktop/tests leave the flag false and end quietly (tests build a fresh runtime
        // per app, where this is a normal shutdown, not a crash).
        if EXIT_ON_SIDECAR_LOSS.load(Ordering::SeqCst) {
            error!("dcl_deno_ipc runtime terminated (JS sidecar lost); exiting process");
            std::process::exit(1);
        }
    });

    init_rx.blocking_recv()?
}

#[allow(clippy::type_complexity)]
pub async fn renderer_ipc_out(
    mut stream: SendHalf,
    mut new_scene: tokio::sync::mpsc::UnboundedReceiver<NewSceneCommand>,
    mut ipc_router: tokio::sync::mpsc::UnboundedReceiver<(u64, IpcMessage)>,
) {
    let (renderer_sx, mut renderer_rx) = tokio::sync::mpsc::unbounded_channel();

    let (_dummy_global_sx, mut global_rx) = tokio::sync::broadcast::channel(1);

    loop {
        tokio::select! {
            new_scene = new_scene.recv() => {
                let Some(NewSceneCommand{id,info,mut renderer_channel, global_channel, response_channel, system_api_sender }) = new_scene else {
                    warn!("renderer_ipc_out exit on new scene closed");
                    return;
                };

                RENDERER_SENDER.set(Some(response_channel));

                if let Some(system_api_sender) = system_api_sender {
                    SYSTEM_API_SENDER.set(Some(system_api_sender));
                }

                // might cause a couple of duplicated global messages for old scenes
                global_rx = global_channel;

                // spawn connector
                let renderer_sender = renderer_sx.clone();
                tokio::spawn(async move {
                    while let Some(renderer_response) = renderer_channel.recv().await {
                        let _ = renderer_sender.send(EngineToScene::SceneUpdate(id, renderer_response));
                    }
                    let _ = renderer_sender.send(EngineToScene::KillScene(id));
                });

                write_msg(&mut stream, &EngineToScene::NewScene(id, Box::new(info))).await;
            }
            renderer_update = renderer_rx.recv() => {
                let Some(engine_to_scene) = renderer_update else {
                    warn!("renderer_ipc_out exit on inbound closed");
                    return;
                };
                write_msg(&mut stream, &engine_to_scene).await;
            }
            global_rx = global_rx.recv() => {
                let data = match global_rx {
                    Ok(data) => data,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                        error!("global crdt state lagged, dropping {count} messages");
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        error!("renderer_ipc_out exit on global crdt closed");
                        return;
                    }
                };
                write_msg(&mut stream, &EngineToScene::GlobalUpdate(data)).await;
            }
            ipc = ipc_router.recv() => {
                let Some(ipc) = ipc else {
                    warn!("renderer_ipc_out exit on router closed");
                    return;
                };
                debug!("ipc {} -> {}", ipc.0, !matches!(ipc.1, IpcMessage::Closed));
                write_msg(&mut stream, &EngineToScene::IpcMessage(ipc.0, ipc.1)).await;
            }
        }
    }
}

pub async fn renderer_ipc_in(mut stream: RecvHalf) {
    while let Ok(len) = stream.read_u64_le().await {
        let mut buffer = vec![0u8; len as usize];
        stream.read_exact(&mut buffer).await.unwrap();
        let msg: SceneToEngine = rmp_serde::from_slice(&buffer).unwrap();

        match msg {
            SceneToEngine::SceneResponse(scene_response) => RENDERER_SENDER.with(|sender| {
                let mut sender = sender.borrow_mut();
                let sender = sender.as_mut().unwrap();
                // HEADLESS-ONLY: EXIT_ON_SIDECAR_LOSS marks the orchestrated headless server,
                // where every scene shares one engine. There, a panic in this IPC task ends
                // the loop and trips the process exit above — killing the whole engine and
                // every co-tenant scene. So shed the response instead of panicking: a scene
                // that outruns the bevy-side drain only stalls itself. Desktop and tests
                // (flag unset) keep the original panic-on-failure behavior unchanged.
                if EXIT_ON_SIDECAR_LOSS.load(Ordering::SeqCst) {
                    if let Err(e) = sender.try_send(scene_response) {
                        warn!("dropping scene response: renderer channel unavailable ({e})");
                    }
                } else {
                    sender.try_send(scene_response).unwrap();
                }
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
    scene_context: CrdtContext,
    scene_js: SceneJsFile,
    crdt_component_interfaces: CrdtComponentInterfaces,
    renderer_sender: SceneResponseSender,
    global_update_receiver: tokio::sync::broadcast::Receiver<GlobalCrdtStateUpdate>,
    storage_root: String,
    inspect: bool,
    super_user: Option<tokio::sync::mpsc::UnboundedSender<SystemApi>>,
    scene_origin: bevy::prelude::Vec3,
) -> tokio::sync::mpsc::UnboundedSender<RendererResponse> {
    let is_super = super_user.is_some();
    let id = scene_context.scene_id;

    let (main_sx, thread_rx) = tokio::sync::mpsc::unbounded_channel::<RendererResponse>();

    let ipc_out = NEW_SCENE_SENDER.read().unwrap();
    let ipc_out = ipc_out.as_ref().unwrap();

    ipc_out
        .send(NewSceneCommand {
            id: id.0.to_bits(),
            info: NewSceneInfo {
                initial_crdt_store,
                scene_context,
                scene_js: scene_js.0.to_string(),
                crdt_component_interfaces,
                storage_root,
                inspect,
                is_super,
                scene_origin,
            },
            renderer_channel: thread_rx,
            global_channel: global_update_receiver,
            response_channel: renderer_sender,
            system_api_sender: super_user,
        })
        .unwrap();

    main_sx
}

pub async fn write_msg<T: Serialize>(stream: &mut SendHalf, value: &T) {
    let bytes = rmp_encode(value).unwrap();
    stream
        .write_all(&(bytes.len() as u64).to_le_bytes())
        .await
        .unwrap();
    stream.write_all(&bytes).await.unwrap();
}
