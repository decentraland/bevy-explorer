use std::time::Duration;

use anyhow::{anyhow, bail};
use bevy::{
    prelude::*,
    tasks::{IoTaskPool, Task},
};
use common::{
    structs::{CurrentRealm, PreviewCommand, PreviewMode},
    util::TaskExt,
};
use dcl_component::proto_components::sdk::development::WsSceneMessage;
use platform::IntoClientRequest;
use prost::Message;

pub struct PreviewPlugin;

impl Plugin for PreviewPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<PreviewCommand>()
            .add_systems(PostUpdate, connect_preview_server.after(ipfs::change_realm));
    }
}

#[allow(clippy::type_complexity)]
fn connect_preview_server(
    mut preview: ResMut<PreviewMode>,
    mut task: Local<
        Option<(
            Task<Result<(), anyhow::Error>>,
            tokio::sync::mpsc::UnboundedReceiver<PreviewCommand>,
        )>,
    >,
    current_realm: Res<CurrentRealm>,
    mut writer: EventWriter<PreviewCommand>,
) {
    let Some(server) = preview.server.as_ref() else {
        preview.is_preview = false;
        return;
    };

    if current_realm.is_changed() {
        *task = None;
    }

    if &current_realm.address != server
        && current_realm.about_url.strip_suffix("/about") != Some(server)
    {
        preview.is_preview = false;
        return;
    }

    let mut restart = task.as_ref().is_none();
    if let Some(res) = task.as_mut().and_then(|t| t.0.complete()) {
        if let Err(err) = res {
            warn!("preview socket error: {err}, restarting");
        }
        restart = true;
        *task = None;
    }
    if restart {
        let (sx, rx) = tokio::sync::mpsc::unbounded_channel();
        *task = Some((
            IoTaskPool::get().spawn(handle_preview_socket(server.clone(), sx)),
            rx,
        ));
    }

    while let Some(command) = task.as_mut().and_then(|(_, rx)| rx.try_recv().ok()) {
        writer.write(command);
    }

    preview.is_preview = true;
}

pub async fn handle_preview_socket(
    server: String,
    sender: tokio::sync::mpsc::UnboundedSender<PreviewCommand>,
) -> Result<(), anyhow::Error> {
    let result = preview_socket(server, sender).await;
    // back off on every exit path, not just a clean disconnect: a preview server with
    // no ws endpoint fails the upgrade immediately and the caller respawns us as soon
    // as the task completes, so an early error spins one attempt per frame
    async_std::task::sleep(Duration::from_secs(5)).await;
    result
}

async fn preview_socket(
    server: String,
    sender: tokio::sync::mpsc::UnboundedSender<PreviewCommand>,
) -> Result<(), anyhow::Error> {
    let (protocol, rest) = server
        .split_once("//")
        .ok_or(anyhow!("invalid preview server address `{server}`"))?;
    let remote_address = if protocol == "http:" {
        format!("ws://{rest}")
    } else if protocol == "https:" {
        format!("wss://{rest}")
    } else {
        bail!("invalid preview server protocol `{protocol}` from `{server}`");
    };

    let request = remote_address.into_client_request()?;
    let stream = platform::websocket(request).await?;
    debug!("preview socket connected");

    let (_, mut read) = stream.split();

    while let Some(msg) = read.next().await {
        let msg = msg?;
        info!("preview server message: {msg}");
        let data = msg.into_data();
        let Ok(ws_scene_message) = WsSceneMessage::decode(data.as_slice()) else {
            debug!("Not a prost message.");
            continue;
        };
        let Some(message) = ws_scene_message.message else {
            debug!("Empty preview message.");
            continue;
        };

        sender.send(message.into())?;
    }

    warn!("preview socket disconnected, waiting 5 secs to attempt reconnect");
    Ok(())
}
