pub mod wallet;
pub mod websocket_room;

use bevy::prelude::*;
use tokio::sync::mpsc::Sender;

use crate::ipfs::CurrentRealm;

use self::websocket_room::{WebsocketRoomAdapter, WebsocketRoomPlugin};

pub struct CommsPlugin;

impl Plugin for CommsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(WebsocketRoomPlugin);
        app.add_system(process_realm_change);
    }
}

pub struct TransportAlias {
    pub adapter: Entity,
    pub alias: u32,
}

#[derive(Component)]
pub struct Peer {
    pub local_id: u32,
    pub transport_aliases: Vec<TransportAlias>,
    pub address: String, // H160
}

pub enum AdapterType {
    WebsocketRoom,
}

pub struct NetworkMessage {
    pub data: Vec<u8>,
    pub reliable: bool,
}

#[derive(Component)]
pub struct Adapter {
    pub adapter_type: AdapterType,
    pub sender: Sender<NetworkMessage>,
}

fn process_realm_change(
    mut commands: Commands,
    realm: Res<CurrentRealm>,
    adapters: Query<Entity, With<Adapter>>,
) {
    if realm.is_changed() {
        for adapter in adapters.iter() {
            commands.entity(adapter).despawn_recursive();
        }

        if let Some(comms) = realm.comms.as_ref() {
            if let Some(fixed_adapter) = comms.fixed_adapter.as_ref() {
                // fixedAdapter	"ws-room:wss://sdk-test-scenes.decentraland.zone/mini-comms/room-1"
                let Some((protocol, address)) = fixed_adapter.split_once(':') else {
                    warn!("unrecognised fixed adapter string: {fixed_adapter}");
                    return;
                };

                match protocol {
                    "ws-room" => {
                        info!("starting ws-room adapter");
                        let (sender, receiver) = tokio::sync::mpsc::channel(1000);

                        commands.spawn((
                            Adapter {
                                adapter_type: AdapterType::WebsocketRoom,
                                sender,
                            },
                            WebsocketRoomAdapter {
                                address: address.to_owned(),
                                receiver,
                            },
                        ));
                    }
                    _ => {
                        warn!("unrecognised fixed adapter protocol: {protocol}");
                    }
                }
            } else {
                warn!("no fixed adapter");
            }
        } else {
            warn!("missing comms!");
        }
    }
}
