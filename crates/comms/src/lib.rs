pub mod broadcast_position;
pub mod global_crdt;
pub mod livekit_room;
pub mod profile;
pub mod signed_login;
#[cfg(test)]
mod test;
pub mod wallet;
pub mod websocket_room;

use std::marker::PhantomData;

use bevy::{
    ecs::{event::ManualEventReader, system::SystemParam},
    prelude::*,
    tasks::{IoTaskPool, Task},
};
use bimap::BiMap;
use ethers::types::Address;
use isahc::http::Uri;
use tokio::sync::mpsc::Sender;

use common::util::TaskExt;
use dcl_component::{proto_components::kernel::comms::rfc4, DclWriter, ToDclWriter};
use ipfs::CurrentRealm;

use self::{
    broadcast_position::BroadcastPositionPlugin,
    global_crdt::GlobalCrdtPlugin,
    livekit_room::{LivekitPlugin, LivekitTransport},
    profile::{CurrentUserProfile, UserProfilePlugin},
    signed_login::{signed_login, SignedLoginMeta, SignedLoginResponse},
    wallet::Wallet,
    websocket_room::{WebsocketRoomPlugin, WebsocketRoomTransport},
};

pub struct CommsPlugin;

impl Plugin for CommsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(WebsocketRoomPlugin);
        app.add_plugins(LivekitPlugin);
        app.add_plugins(BroadcastPositionPlugin);
        app.add_plugins(GlobalCrdtPlugin);
        app.add_plugins(UserProfilePlugin);
        app.add_systems(
            Update,
            (
                process_realm_change,
                start_ws_room,
                start_signed_login,
                start_livekit,
            ),
        );
        app.add_event::<StartWsRoom>();
        app.add_event::<StartLivekit>();
        app.add_event::<StartSignedLogin>();
    }
}

pub struct TransportAlias {
    pub adapter: Entity,
    pub alias: u32,
}

pub enum TransportType {
    WebsocketRoom,
    Livekit,
}

pub struct NetworkMessage {
    pub data: Vec<u8>,
    pub unreliable: bool,
}

impl NetworkMessage {
    pub fn unreliable<D: ToDclWriter>(message: &D) -> Self {
        let mut data = Vec::new();
        let mut writer = DclWriter::new(&mut data);
        message.to_writer(&mut writer);
        Self {
            data,
            unreliable: true,
        }
    }

    pub fn reliable<D: ToDclWriter>(message: &D) -> Self {
        Self {
            unreliable: false,
            ..Self::unreliable(message)
        }
    }
}

#[derive(Component)]
pub struct Transport {
    pub transport_type: TransportType,
    pub sender: Sender<NetworkMessage>,
    pub foreign_aliases: BiMap<u32, Address>,
}

fn process_realm_change(
    mut commands: Commands,
    realm: Res<CurrentRealm>,
    adapters: Query<Entity, With<Transport>>,
    mut manager: AdapterManager,
) {
    if realm.is_changed() {
        for adapter in adapters.iter() {
            commands.entity(adapter).despawn_recursive();
        }

        if let Some(comms) = realm.comms.as_ref() {
            if let Some(fixed_adapter) = comms.fixed_adapter.as_ref() {
                manager.connect(fixed_adapter);
            } else {
                warn!("no fixed adapter, i don't understand anything else");
            }
        } else {
            debug!("missing comms!");
        }
    }
}

#[derive(Event)]
pub struct StartWsRoom {
    address: String,
}

pub fn start_ws_room(
    mut commands: Commands,
    mut room_events: EventReader<StartWsRoom>,
    current_profile: Res<CurrentUserProfile>,
) {
    if let Some(ev) = room_events.iter().last() {
        info!("starting ws-room protocol");
        let (sender, receiver) = tokio::sync::mpsc::channel(1000);

        // queue a profile version message
        let response = rfc4::Packet {
            message: Some(rfc4::packet::Message::ProfileVersion(
                rfc4::AnnounceProfileVersion {
                    profile_version: current_profile.0.version,
                },
            )),
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

#[derive(Event)]
pub struct StartSignedLogin {
    address: String,
}

pub fn start_signed_login(
    mut signed_login_events: Local<ManualEventReader<StartSignedLogin>>,
    current_realm: Res<CurrentRealm>,
    wallet: Res<Wallet>,
    mut task: Local<Option<Task<Result<SignedLoginResponse, anyhow::Error>>>>,
    mut manager: AdapterManager,
) {
    if let Some(ev) = signed_login_events
        .iter(&manager.signed_login_events)
        .last()
    {
        info!("starting signed login");
        let address = ev.address.clone();
        let Ok(uri) = Uri::try_from(&address) else {
            warn!("failed to parse signed login address as a uri: {address}");
            return;
        };
        let wallet = wallet.clone();
        let Ok(origin) = Uri::try_from(&current_realm.address) else {
            warn!("failed to parse signed login address as a uri: {address}");
            return;
        };

        let meta = SignedLoginMeta::new(true, origin);
        *task = Some(IoTaskPool::get().spawn(signed_login(uri, wallet, meta)));
    }

    if let Some(mut current_task) = task.take() {
        if let Some(result) = current_task.complete() {
            match result {
                Ok(SignedLoginResponse {
                    fixed_adapter: Some(adapter),
                    ..
                }) => {
                    info!("signed login ok, connecting to inner {adapter}");
                    manager.connect(adapter.as_str())
                }
                otherwise => warn!("signed login failed: {otherwise:?}"),
            }
        } else {
            *task = Some(current_task);
        }
    }
}

#[derive(Event)]
pub struct StartLivekit {
    address: String,
}

pub fn start_livekit(
    mut commands: Commands,
    mut room_events: EventReader<StartLivekit>,
    current_profile: Res<CurrentUserProfile>,
) {
    if let Some(ev) = room_events.iter().last() {
        info!("starting livekit protocol");
        let (sender, receiver) = tokio::sync::mpsc::channel(1000);

        // queue a profile version message
        let response = rfc4::Packet {
            message: Some(rfc4::packet::Message::ProfileVersion(
                rfc4::AnnounceProfileVersion {
                    profile_version: current_profile.0.version,
                },
            )),
        };
        let _ = sender.try_send(NetworkMessage::reliable(&response));

        commands.spawn((
            Transport {
                transport_type: TransportType::Livekit,
                sender,
                foreign_aliases: Default::default(),
            },
            LivekitTransport {
                address: ev.address.to_owned(),
                receiver: Some(receiver),
                retries: 0,
            },
        ));
    }
}

#[derive(SystemParam)]
pub struct AdapterManager<'w, 's> {
    ws_room_events: EventWriter<'w, StartWsRoom>,
    livekit_events: EventWriter<'w, StartLivekit>,
    // can't use event writer due to conflict on Res<Events>
    pub signed_login_events: ResMut<'w, Events<StartSignedLogin>>,
    #[system_param(ignore)]
    _p: PhantomData<&'s ()>,
}

impl<'w, 's> AdapterManager<'w, 's> {
    pub fn connect(&mut self, adapter: &str) {
        let Some((protocol, address)) = adapter.split_once(':') else {
            warn!("unrecognised fixed adapter string: {adapter}");
            return;
        };

        match protocol {
            "ws-room" => self.ws_room_events.send(StartWsRoom {
                address: address.to_owned(),
            }),
            "signed-login" => self.signed_login_events.send(StartSignedLogin {
                address: address.to_owned(),
            }),
            "livekit" => self.livekit_events.send(StartLivekit {
                address: address.to_owned(),
            }),
            "offline" => {
                info!("comms offline");
            }
            _ => {
                warn!("unrecognised adapter protocol: {protocol}");
            }
        }
    }
}
