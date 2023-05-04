use anyhow::bail;
use async_std::net::TcpStream;
use async_tls::client::TlsStream;
use async_tungstenite::{stream::Stream, tungstenite::client::IntoClientRequest, WebSocketStream};
use bevy::{
    prelude::*,
    tasks::{IoTaskPool, Task},
};
use futures_lite::future;
use futures_util::{stream::StreamExt, SinkExt};
use isahc::http::HeaderValue;
use prost::Message;
use tokio::sync::mpsc::Receiver;

use crate::dcl_component::proto_components::kernel::comms::rfc5::{
    ws_packet, WsChallengeRequired, WsIdentification, WsPacket, WsSignedChallenge, WsWelcome,
};

use super::{
    wallet::{SimpleAuthChain, Wallet},
    NetworkMessage,
};

pub struct WebsocketRoomPlugin;

impl Plugin for WebsocketRoomPlugin {
    fn build(&self, app: &mut App) {
        app.add_system(connect_websocket);
        app.add_system(process_connecting);
    }
}

#[derive(Component)]
pub struct WebsocketRoomAdapter {
    pub address: String,
    pub receiver: Receiver<NetworkMessage>,
}

type WssStream = WebSocketStream<Stream<TcpStream, TlsStream<TcpStream>>>;

#[derive(Component)]
pub struct WebSocketInitTask(Task<Result<(WssStream, WsWelcome), anyhow::Error>>);

#[derive(Component)]
pub enum WebSocketConnection {
    Ready(WssStream),
    Failed,
}

#[allow(clippy::type_complexity)]
fn connect_websocket(
    mut commands: Commands,
    new_adapters: Query<
        (Entity, &WebsocketRoomAdapter),
        (Without<WebSocketConnection>, Without<WebSocketInitTask>),
    >,
    wallet: Res<Wallet>,
) {
    for (entity, new_adapter) in &new_adapters {
        let remote_address = new_adapter.address.to_owned();
        let wallet = wallet.clone();
        let task = IoTaskPool::get().spawn(async move {
            debug!(">> stream connect async : {remote_address}");

            let mut request = remote_address.into_client_request()?;
            request.headers_mut().append("Sec-WebSocket-Protocol", HeaderValue::from_static("rfc5"));

            let (mut stream, response) = async_tungstenite::async_std::connect_async(request).await?;
            debug!("<< stream connected, response: {response:?}");

            // send peer identification
            let ident = WsPacket {
                message: Some(ws_packet::Message::PeerIdentification(WsIdentification {
                    address: format!("{:#x}", wallet.address()),
                })),
            };
            stream.send(ident.encode_to_vec().into()).await?;
            debug!(">> ident sent: {ident:?}");

            // receive challenge
            let Some(response) = stream.next().await else { bail!("stream closed unexpectedly awaiting challenge") };
            let response = response?;
            let response = WsPacket::decode(response.into_data().as_slice())?;
            let WsPacket{ message: Some(ws_packet::Message::ChallengeMessage(WsChallengeRequired{ challenge_to_sign, .. })) } = response else { bail!("expected challenge, received: {response:?}") };
            debug!("<< challenge received; {challenge_to_sign}");

            // sign challenge
            let signature = wallet.sign_message(challenge_to_sign.as_bytes()).await?;
            let chain = SimpleAuthChain::new(wallet.address(), challenge_to_sign, signature);
            let auth_chain_json = serde_json::to_string(&chain)?;
            debug!(">> auth chain created: {auth_chain_json}");

            // send response
            let message = WsPacket{ message: Some(ws_packet::Message::SignedChallengeForServer(WsSignedChallenge { auth_chain_json })) };
            let message = message.encode_to_vec();
            stream.send(message.into()).await?;
            println!(">> auth chain sent");

            // receive welcome
            let Some(response) = stream.next().await else { bail!("stream closed unexpectedly awaiting welcome") };
            let response = response?;
            let response = WsPacket::decode(response.into_data().as_slice())?;
            let WsPacket{ message: Some(ws_packet::Message::WelcomeMessage(welcome)) } = response else { bail!("expected welcome, received: {response:?}") };
            println!("<< welcome received: {welcome:?}");

            Ok((stream, welcome))
        });

        commands.entity(entity).insert(WebSocketInitTask(task));
    }
}

fn process_connecting(
    mut commands: Commands,
    mut connecting_adapters: Query<(Entity, &mut WebSocketInitTask)>,
) {
    for (entity, mut task) in connecting_adapters.iter_mut() {
        if task.0.is_finished() {
            match future::block_on(future::poll_once(&mut task.0)).unwrap() {
                Ok((stream, welcome)) => {
                    info!("websocket connected: {welcome:?}");
                    commands
                        .entity(entity)
                        .remove::<WebSocketInitTask>()
                        .insert(WebSocketConnection::Ready(stream));
                }
                Err(e) => {
                    warn!("websocket connection failed: {e}");
                    commands
                        .entity(entity)
                        .remove::<WebSocketInitTask>()
                        .insert(WebSocketConnection::Failed);
                }
            }
        }
    }
}
