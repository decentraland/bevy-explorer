use std::{str::FromStr, time::Duration};

use anyhow::{anyhow, bail};
use async_tungstenite::tungstenite::client::IntoClientRequest;
use bevy::{
    prelude::*,
    tasks::{IoTaskPool, Task},
};
use common::util::TaskExt;
use futures_util::StreamExt;
use ipfs::CurrentRealm;

#[derive(Resource, Default)]
pub struct PreviewMode {
    pub server: Option<String>,
    pub is_preview: bool,
}

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

    if &current_realm.address != server {
        preview.is_preview = false;
        return;
    }

    if task.as_ref().map_or(true, |(t, _)| t.is_finished()) {
        if let Some(Err(e)) = task.take().map(|(mut t, _)| t.complete().unwrap()) {
            warn!("preview socket error: {e}");
        };
        let (sx, rx) = tokio::sync::mpsc::unbounded_channel();
        *task = Some((
            IoTaskPool::get().spawn(handle_preview_socket(server.clone(), sx)),
            rx,
        ));
    }

    while let Some(command) = task.as_mut().and_then(|(_, rx)| rx.try_recv().ok()) {
        writer.send(command);
    }

    preview.is_preview = true;
}

#[derive(Event)]
pub enum PreviewCommand {
    ReloadScene { hash: String },
}

pub async fn handle_preview_socket(
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
    let (stream, response) = async_tungstenite::async_std::connect_async(request).await?;
    debug!("preview socket connected, response: {response:?}");

    let (_, mut read) = stream.split();

    while let Some(msg) = read.next().await {
        let msg = msg?;
        info!("preview server message: {msg}");

        if let Ok(value) = serde_json::Value::from_str(msg.into_text()?.as_str()) {
            let Some(ty) = value
                .get("type")
                .and_then(|v| v.as_str().map(ToOwned::to_owned))
            else {
                continue;
            };

            #[allow(clippy::single_match)] // we will handle more messages in future
            match ty.as_str() {
                "SCENE_UPDATE" => {
                    if let Some(hash) = value
                        .get("payload")
                        .and_then(|payload| payload.get("sceneId"))
                        .and_then(|scene_id| scene_id.as_str().map(ToOwned::to_owned))
                    {
                        sender.send(PreviewCommand::ReloadScene { hash })?;
                    } else {
                        warn!("malformed scene update");
                    }
                }
                _ => (),
            }
        }
    }

    warn!("preview socket disconnected, waiting 5 secs to attempt reconnect");
    async_std::task::sleep(Duration::from_secs(5)).await;
    Ok(())
}
