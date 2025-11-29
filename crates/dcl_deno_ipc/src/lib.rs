use bevy::log::{error, info};
use common::rpc::IpcMessage;
use dcl::{interface::CrdtComponentInterfaces, RendererResponse, SceneId, SceneResponse};
use interprocess::local_socket::{GenericFilePath, ListenerOptions, Stream, ToFsName, tokio::{RecvHalf, SendHalf}, traits::tokio::{Listener, Stream as _}};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use std::{
    process::{Command, Stdio},
    sync::Mutex,
};

#[derive(Serialize, Deserialize)]
pub enum EngineToScene {
    NewScene {
        scene_hash: String,
        scene_js: String,
        crdt_component_interfaces: CrdtComponentInterfaces,
        id: SceneId,
        storage_root: String,
        inspect: bool,
        testing: bool,
        preview: bool,
        super_user: bool,
    },
    SceneUpdate(RendererResponse),
    GlobalUpdate(Vec<u8>),
    IpcMessage(IpcMessage),
}

pub enum SceneToEngine {
    SceneResponse(SceneResponse),
    IpcMessage(IpcMessage),
}

pub static IPC_OUT: Lazy<Mutex<Option<tokio::sync::mpsc::UnboundedSender<EngineToScene>>>> =
    Lazy::new(Default::default);

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
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();

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

        let (engine_sx, engine_rx) = tokio::sync::mpsc::unbounded_channel();

        *IPC_OUT.lock().unwrap() = Some(engine_sx);

        let f_out = rt.spawn(renderer_ipc_out(ipc_outbound, engine_rx));
        let f_in = rt.spawn(renderer_ipc_in(ipc_inbound));

        let _ = init_sx.send(Ok(()));

        let _ = rt.block_on(async move { tokio::join!(f_out, f_in) });
    });

    init_rx.blocking_recv()?
}

pub async fn renderer_ipc_out(mut stream: SendHalf, mut inbound: tokio::sync::mpsc::UnboundedReceiver<EngineToScene>) {
    while let Some(e2s) = inbound.recv().await {
        let bytes = bincode::serialize(&e2s).unwrap();
        stream
            .write_all(&(bytes.len() as u64).to_le_bytes())
            .await
            .unwrap();

    }
}

pub async fn renderer_ipc_in(mut stream: RecvHalf) {

}