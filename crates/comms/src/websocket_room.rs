use anyhow::{anyhow, bail};
use async_tungstenite::tungstenite::{client::IntoClientRequest, http::HeaderValue};
use bevy::{
    prelude::*,
    tasks::{IoTaskPool, Task},
};
use bimap::BiMap;
use futures_lite::future;
use futures_util::{pin_mut, select, stream::StreamExt, FutureExt, SinkExt};
use prost::Message;
use tokio::sync::mpsc::{Receiver, Sender};

use common::util::{dcl_assert, AsH160};
use dcl_component::proto_components::kernel::comms::{
    rfc4,
    rfc5::{
        ws_packet, WsChallengeRequired, WsIdentification, WsPacket, WsPeerUpdate, WsSignedChallenge,
    },
};
use wallet::Wallet;

use crate::{global_crdt::PlayerMessage, profile::CurrentUserProfile, Transport, TransportType};

use super::{
    global_crdt::{GlobalCrdtState, PlayerUpdate},
    NetworkMessage,
};

pub struct WebsocketRoomPlugin;

impl Plugin for WebsocketRoomPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (connect_websocket, reconnect_websocket, start_ws_room),
        );
        app.add_event::<StartWsRoom>();
    }
}

#[derive(Event)]
pub struct StartWsRoom {
    pub address: String,
}

#[derive(Component)]
pub struct WebsocketRoomTransport {
    pub address: String,
    pub receiver: Option<Receiver<NetworkMessage>>,
    pub retries: usize,
}

#[derive(Component)]
pub struct WebSocketConnection(Task<(Receiver<NetworkMessage>, anyhow::Error)>);

pub fn start_ws_room(
    mut commands: Commands,
    mut room_events: EventReader<StartWsRoom>,
    current_profile: Res<CurrentUserProfile>,
) {
    if let Some(ev) = room_events.read().last() {
        info!("starting ws-room protocol");
        let (sender, receiver) = tokio::sync::mpsc::channel(1000);

        let Some(current_profile) = current_profile.profile.as_ref() else {
            return;
        };

        // queue a profile version message
        let response = rfc4::Packet {
            message: Some(rfc4::packet::Message::ProfileVersion(
                rfc4::AnnounceProfileVersion {
                    profile_version: current_profile.version,
                },
            )),
            protocol_version: 999,
        };
        let _ = sender.try_send(NetworkMessage::reliable(&response));

        commands.spawn((
            Transport {
                transport_type: TransportType::WebsocketRoom,
                sender,
                foreign_aliases: Default::default(),
            },
            WebsocketRoomTransport {
                address: ev.address.to_owned(),
                receiver: Some(receiver),
                retries: 0,
            },
        ));
    }
}

#[allow(clippy::type_complexity)]
fn connect_websocket(
    mut commands: Commands,
    mut new_websockets: Query<(Entity, &mut WebsocketRoomTransport), Without<WebSocketConnection>>,
    wallet: Res<Wallet>,
    player_state: Res<GlobalCrdtState>,
) {
    for (transport_id, mut new_transport) in new_websockets.iter_mut() {
        let remote_address = new_transport.address.to_owned();
        let wallet = wallet.clone();
        let receiver = new_transport.receiver.take().unwrap();
        let sender = player_state.get_sender();
        let task = IoTaskPool::get().spawn(websocket_room_handler(
            transport_id,
            remote_address,
            wallet,
            receiver,
            sender,
        ));
        commands
            .entity(transport_id)
            .try_insert(WebSocketConnection(task));
    }
}

fn reconnect_websocket(
    mut websockets: Query<(
        Entity,
        &mut WebsocketRoomTransport,
        &mut WebSocketConnection,
    )>,
    wallet: Res<Wallet>,
    player_state: Res<GlobalCrdtState>,
) {
    for (transport_id, mut transport, mut conn) in websockets.iter_mut() {
        if transport.retries < 3 {
            if conn.0.is_finished() {
                transport.retries += 1;
                let (receiver, err) = future::block_on(future::poll_once(&mut conn.0)).unwrap();
                warn!(
                    "websocket room error: {err}, retrying [{}]",
                    transport.address
                );
                let remote_address = transport.address.to_owned();
                let wallet = wallet.clone();
                let sender = player_state.get_sender();
                let task = IoTaskPool::get().spawn(websocket_room_handler(
                    transport_id,
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
    transport_id: Entity,
    remote_address: String,
    wallet: Wallet,
    mut receiver: Receiver<NetworkMessage>,
    sender: Sender<PlayerUpdate>,
) -> (Receiver<NetworkMessage>, anyhow::Error) {
    let res =
        websocket_room_handler_inner(transport_id, remote_address, wallet, &mut receiver, sender)
            .await;
    (receiver, res.err().unwrap_or(anyhow!("connection closed")))
}

async fn websocket_room_handler_inner(
    transport_id: Entity,
    remote_address: String,
    wallet: Wallet,
    receiver: &mut Receiver<NetworkMessage>,
    sender: Sender<PlayerUpdate>,
) -> Result<(), anyhow::Error> {
    debug!(">> stream connect async : {remote_address}");

    let remote_address = if remote_address.starts_with("ws:") || remote_address.starts_with("wss:")
    {
        remote_address
    } else {
        format!("wss://{remote_address}")
    };

    let mut request = remote_address.into_client_request()?;
    request
        .headers_mut()
        .append("Sec-WebSocket-Protocol", HeaderValue::from_static("rfc5"));

    let (mut stream, response) = async_tungstenite::async_std::connect_async(request).await?;
    debug!("<< stream connected, response: {response:?}");

    // send peer identification
    let ident = WsPacket {
        message: Some(ws_packet::Message::PeerIdentification(WsIdentification {
            address: format!(
                "{:#x}",
                wallet.address().ok_or(anyhow!("wallet not connected"))?
            ),
        })),
    };
    stream.send(ident.encode_to_vec().into()).await?;
    debug!(">> ident sent: {ident:?}");

    // challenge / welcome
    let from_alias;
    let mut foreign_aliases;
    loop {
        let Some(response) = stream.next().await else {
            bail!("stream closed unexpectedly awaiting challenge")
        };
        let response = response?;
        let response = WsPacket::decode(response.into_data().as_slice())?;
        let Some(message) = response.message else {
            bail!("received empty packet")
        };

        match message {
            ws_packet::Message::WelcomeMessage(welcome) => {
                debug!("<< welcome received: {welcome:?}");
                from_alias = welcome.alias;
                foreign_aliases = BiMap::from_iter(welcome.peer_identities.into_iter().flat_map(
                    |(alias, address)| {
                        if let Some(h160) = address.as_h160() {
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

                if !challenge_to_sign.starts_with("dcl-") {
                    error!("invalid challenge to sign");
                    return Err(anyhow!("invalid challenge to sign"));
                }

                // sign challenge
                let chain = wallet.sign_message(challenge_to_sign).await?;
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
            let Some(message) = next.message else {
                bail!("received empty packet")
            };

            match message {
                ws_packet::Message::ChallengeMessage(_)
                | ws_packet::Message::PeerIdentification(_)
                | ws_packet::Message::SignedChallengeForServer(_)
                | ws_packet::Message::WelcomeMessage(_) => {
                    warn!("unexpected bau message: {message:?}");
                    continue;
                }
                ws_packet::Message::PeerJoinMessage(peer) => {
                    debug!("peer joined: {} -> {}", peer.alias, peer.address);
                    if let Some(h160) = peer.address.as_h160() {
                        foreign_aliases.insert(peer.alias, h160);
                    } else {
                        warn!("failed to parse hash: {}", peer.address);
                    }
                }
                ws_packet::Message::PeerLeaveMessage(peer) => {
                    debug!(
                        "peer left: {} -> {:?}",
                        peer.alias,
                        foreign_aliases.get_by_left(&peer.alias)
                    );
                    foreign_aliases.remove_by_left(&peer.alias);
                }
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

                    let Some(address) = foreign_aliases.get_by_left(&update.from_alias).cloned()
                    else {
                        debug!("received packet for unknown alias {}", update.from_alias);
                        continue;
                    };

                    debug!(
                        "[tid: {:?}] received message {:?} from {:?}",
                        transport_id, message, address
                    );
                    sender
                        .send(PlayerUpdate {
                            transport_id,
                            message: PlayerMessage::PlayerData(message),
                            address,
                        })
                        .await
                        .map_err(|_| anyhow!("Send error"))?;
                }
                ws_packet::Message::PeerKicked(reason) => {
                    warn!("kicked: {}", reason.reason);
                    return Ok(());
                }
            }
        }

        Ok(())
    }
    .fuse();

    // until either stream is broken
    pin_mut!(f_read, f_write);
    select! {
        read_res = f_read => read_res,
        write_res = f_write => write_res,
    }
}
