use anyhow::{anyhow, bail};
use async_std::net::TcpStream;
use async_tls::client::TlsStream;
use async_tungstenite::{stream::Stream, tungstenite::client::IntoClientRequest, WebSocketStream};
use bevy::{
    prelude::*,
    tasks::{IoTaskPool, Task},
};
use bimap::BiMap;
use futures_lite::future;
use futures_util::{pin_mut, select, stream::StreamExt, FutureExt, SinkExt};
use isahc::http::HeaderValue;
use prost::Message;
use tokio::sync::mpsc::{Receiver, Sender};

use crate::{
    comms::AsH160,
    dcl_assert,
    dcl_component::proto_components::kernel::comms::{
        rfc4,
        rfc5::{
            ws_packet, WsChallengeRequired, WsIdentification, WsPacket, WsPeerUpdate,
            WsSignedChallenge, WsWelcome,
        },
    },
};

use super::{
    foreign_player::{ForeignPlayerState, PlayerUpdate},
    wallet::{SimpleAuthChain, Wallet},
    NetworkMessage,
};

pub struct WebsocketRoomPlugin;

impl Plugin for WebsocketRoomPlugin {
    fn build(&self, app: &mut App) {
        app.add_system(connect_websocket);
        app.add_system(reconnect_websocket);
    }
}

#[derive(Component)]
pub struct WebsocketRoomTransport {
    pub address: String,
    pub receiver: Option<Receiver<NetworkMessage>>,
    pub retries: usize,
}

type WssStream = WebSocketStream<Stream<TcpStream, TlsStream<TcpStream>>>;

#[derive(Component)]
pub struct WebSocketInitTask(Task<Result<(WssStream, WsWelcome), anyhow::Error>>);

#[derive(Component)]
pub struct WebSocketConnection(Task<(Receiver<NetworkMessage>, anyhow::Error)>);

#[allow(clippy::type_complexity)]
fn connect_websocket(
    mut commands: Commands,
    mut new_websockets: Query<(Entity, &mut WebsocketRoomTransport), Without<WebSocketConnection>>,
    wallet: Res<Wallet>,
    player_state: Res<ForeignPlayerState>,
) {
    for (entity, mut new_transport) in new_websockets.iter_mut() {
        let remote_address = new_transport.address.to_owned();
        let wallet = wallet.clone();
        let receiver = new_transport.receiver.take().unwrap();
        let sender = player_state.get_sender();
        let task = IoTaskPool::get().spawn(websocket_room_handler(
            remote_address,
            wallet,
            receiver,
            sender,
        ));
        commands.entity(entity).insert(WebSocketConnection(task));
    }
}

fn reconnect_websocket(
    mut websockets: Query<(&mut WebsocketRoomTransport, &mut WebSocketConnection)>,
    wallet: Res<Wallet>,
    player_state: Res<ForeignPlayerState>,
) {
    for (mut transport, mut conn) in websockets.iter_mut() {
        if transport.retries < 3 {
            if conn.0.is_finished() {
                transport.retries += 1;
                let (receiver, err) = future::block_on(future::poll_once(&mut conn.0)).unwrap();
                warn!("websocket room error: {err}, retrying");
                let remote_address = transport.address.to_owned();
                let wallet = wallet.clone();
                let sender = player_state.get_sender();
                let task = IoTaskPool::get().spawn(websocket_room_handler(
                    remote_address,
                    wallet,
                    receiver,
                    sender,
                ));
                conn.0 = task;
            }
        } else if transport.retries == 3 && conn.0.is_finished() {
            transport.retries += 1;
            let (_, err) = future::block_on(future::poll_once(&mut conn.0)).unwrap();
            warn!("websocket room error: {err}, giving up");
        }
    }
}

async fn websocket_room_handler(
    remote_address: String,
    wallet: Wallet,
    mut receiver: Receiver<NetworkMessage>,
    sender: Sender<PlayerUpdate>,
) -> (Receiver<NetworkMessage>, anyhow::Error) {
    let res = websocket_room_handler_inner(remote_address, wallet, &mut receiver, sender).await;
    (receiver, res.err().unwrap_or(anyhow!("connection closed")))
}

async fn websocket_room_handler_inner(
    remote_address: String,
    wallet: Wallet,
    receiver: &mut Receiver<NetworkMessage>,
    sender: Sender<PlayerUpdate>,
) -> Result<(), anyhow::Error> {
    debug!(">> stream connect async : {remote_address}");

    let mut request = remote_address.into_client_request()?;
    request
        .headers_mut()
        .append("Sec-WebSocket-Protocol", HeaderValue::from_static("rfc5"));

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

    // challenge / welcome
    let from_alias;
    let mut foreign_aliases;
    loop {
        let Some(response) = stream.next().await else { bail!("stream closed unexpectedly awaiting challenge") };
        let response = response?;
        let response = WsPacket::decode(response.into_data().as_slice())?;
        let Some(message) = response.message else { bail!("received empty packet") };

        match message {
            ws_packet::Message::WelcomeMessage(welcome) => {
                debug!("<< welcome received: {welcome:?}");
                from_alias = welcome.alias;
                foreign_aliases = BiMap::from_iter(welcome.peer_identities.into_iter().flat_map(
                    |(alias, address)| {
                        if let Some(h160) = (&address[2..]).as_h160() {
                            Some((alias, h160))
                        } else {
                            warn!("failed to parse hash: {}", address);
                            None
                        }
                    },
                ));
                break;
            }
            ws_packet::Message::ChallengeMessage(WsChallengeRequired {
                challenge_to_sign, ..
            }) => {
                // send challenge response
                debug!("<< challenge received; {challenge_to_sign}");

                // sign challenge
                let signature = wallet.sign_message(challenge_to_sign.as_bytes()).await?;
                let chain = SimpleAuthChain::new(wallet.address(), challenge_to_sign, signature);
                let auth_chain_json = serde_json::to_string(&chain)?;
                debug!(">> auth chain created: {auth_chain_json}");

                // send response
                let message = WsPacket {
                    message: Some(ws_packet::Message::SignedChallengeForServer(
                        WsSignedChallenge { auth_chain_json },
                    )),
                };
                let message = message.encode_to_vec();
                stream.send(message.into()).await?;
                debug!(">> auth chain sent");
            }
            _ => bail!("unexpected message during handshake: {message:?}"),
        }
    }
    dcl_assert!(from_alias != u32::MAX);

    let (mut write, mut read) = stream.split();

    // wrap and transmit outbound messages
    let f_write = async move {
        while let Some(next) = receiver.recv().await {
            let packet = WsPacket {
                message: Some(ws_packet::Message::PeerUpdateMessage(WsPeerUpdate {
                    from_alias,
                    body: next.data,
                    unreliable: next.unreliable,
                })),
            };
            let mut buf = Vec::default();
            packet.encode(&mut buf)?;
            write.send(buf.into()).await?;
        }

        Ok::<(), anyhow::Error>(())
    }
    .fuse();

    // unwrap and forward inbound messages
    let f_read = async move {
        while let Some(next) = read.next().await {
            let next = next?;
            let next = WsPacket::decode(next.into_data().as_slice())?;
            let Some(message) = next.message else { bail!("received empty packet") };

            match message {
                ws_packet::Message::ChallengeMessage(_) |
                ws_packet::Message::PeerIdentification(_) => todo!(),
                ws_packet::Message::SignedChallengeForServer(_) => todo!(),
                ws_packet::Message::WelcomeMessage(_) => {
                    warn!("unexpected bau message: {message:?}");
                    continue;
                }
                ws_packet::Message::PeerJoinMessage(peer) => {
                    debug!("peer joined: {} -> {}", peer.alias, peer.address);
                    if let Some(h160) = (&peer.address[2..]).as_h160() {
                        foreign_aliases.insert(peer.alias, h160);
                    } else {
                        warn!("failed to parse hash: {}", peer.address);
                    }
                },
                ws_packet::Message::PeerLeaveMessage(peer) => {
                    debug!("peer left: {} -> {:?}", peer.alias, foreign_aliases.get_by_left(&peer.alias));
                    foreign_aliases.remove_by_left(&peer.alias);
                },
                ws_packet::Message::PeerUpdateMessage(update) => {
                    let packet = match rfc4::Packet::decode(update.body.as_slice()) {
                        Ok(packet) => packet,
                        Err(e) => {
                            warn!("unable to parse packet body: {e}");
                            continue;
                        }
                    };
                    let Some(message) = packet.message else {
                        warn!("received empty packet body");
                        continue;
                    };

                    let Some(address) = foreign_aliases.get_by_left(&update.from_alias).cloned() else {
                        warn!("received packet for unknown alias {}", update.from_alias);
                        continue;
                    };

                    sender.send(PlayerUpdate {
                        message,
                        address,
                    }).await?;
                },
                ws_packet::Message::PeerKicked(reason) => {
                    warn!("kicked: {}", reason.reason);
                    return Ok(());
                },
            }
        }

        Ok(())
    }.fuse();

    // until either stream is broken
    pin_mut!(f_read, f_write);
    select! {
        read_res = f_read => read_res,
        write_res = f_write => write_res,
    }
}
