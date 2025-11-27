/*use common::{rpc::IpcMessage, structs::MicState};
use dcl::{RendererResponse, SceneId, SceneResponse, interface::CrdtComponentInterfaces};
use interprocess::{local_socket::{GenericFilePath, ListenerOptions, ToFsName, traits::Listener}};
use ipfs::SceneJsFile;
use std::process::{Command, Stdio};
use dcl_deno_ipc_types::{IpcSceneResponse, IpcSerial};

pub enum EngineToScene {
    NewScene {
        scene_hash: String,
        scene_js: SceneJsFile,
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

pub fn init_client() -> anyhow::Result<()> {
    let name_str = if cfg!(windows) { "bevy_explorer_ipc" } else { "/tmp/bevy_explorer_ipc.sock" };
    let name = name_str.to_fs_name::<GenericFilePath>()?;

    // 2. Bind the Listener
    let listener = ListenerOptions::new().name(name).create_sync()?;
    
    // 3. Spawn Worker
    let mut _child = Command::new("target/debug/dcl_deno")
        .arg(name_str)
        .stdout(Stdio::inherit()) // <--- Worker writes directly to host console
        .stderr(Stdio::inherit())
        .spawn()?;

    println!("[Host] Waiting for worker connection...");

    // 4. Accept Connection
    let mut stream = listener.accept()?;

    std::thread::spawn(|| {
        let rt = tokio::runtime::Builder::new_current_thread();

        

        // 6. Use the Channel
        stream.send_msg(&Message{ num: 1, str: "hello!".to_string() })?;

        let msg = stream.recv_msg()?;
        println!("[Host] Received: {:?}", msg);

        println!("[main] waiting for child");
        drop(stream);
        let result = _child.wait()?;
        println!("[main] child result: {result}");
        
        println!("[main] exiting");
    });

    Ok(())
}*/