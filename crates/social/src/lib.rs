#[cfg(not(feature = "social"))]
mod fake_client;
#[cfg(not(feature = "social"))]
pub use fake_client::{FriendshipEventBody, SocialClientHandler};

#[cfg(feature = "social")]
mod client;
#[cfg(feature = "social")]
mod rpc_websocket;
#[cfg(feature = "social")]
pub mod runtime;
#[cfg(feature = "social")]
pub use client::{FriendshipEventBody, SocialClientHandler};

use bevy::prelude::*;
#[cfg(feature = "social")]
use bevy::tasks::IoTaskPool;
#[cfg(feature = "social")]
use bevy_console::ConsoleCommand;
use common::rpc::RpcStreamSender;
#[cfg(feature = "social")]
use common::structs::DebugInfo;
use common::util::AsH160;
#[cfg(feature = "social")]
use console::DoAddConsoleCommand;
use ethers_core::types::Address;
#[cfg(feature = "social")]
use system_bridge::BlockedUserData;
#[cfg(feature = "social")]
use system_bridge::NameColor;
use system_bridge::{
    FriendConnectivityEvent, FriendData, FriendRequestData, FriendStatusData,
    FriendshipEventUpdate, SystemApi,
};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};
use wallet::Wallet;

pub struct SocialPlugin;

impl Plugin for SocialPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        #[cfg(feature = "social")]
        app.add_plugins(runtime::SocialRuntimePlugin);
        app.add_event::<FriendshipEvent>();
        app.add_event::<ConnectivityEvent>();
        app.add_event::<DirectChatEvent>();
        app.init_resource::<SocialClient>();
        app.add_systems(PostUpdate, |mut client: ResMut<SocialClient>| {
            if let Some(client) = client.0.as_mut() {
                client.update();
            }
        });
        app.add_systems(PostUpdate, init_social_client);
        app.add_systems(
            PostUpdate,
            (
                handle_social_requests,
                pipe_friendship_events_to_scene,
                pipe_connectivity_events_to_scene,
            ),
        );
        #[cfg(feature = "social")]
        {
            app.init_resource::<DebugSocialEnabled>();
            app.init_resource::<RestartSocialRequested>();
            app.add_console_command::<DebugSocialCommand, _>(toggle_debug_social);
            app.add_console_command::<RestartSocialCommand, _>(restart_social);
            app.add_systems(
                PostUpdate,
                debug_write_social.run_if(|e: Res<DebugSocialEnabled>| e.0),
            );
        }
    }
}

#[cfg(feature = "social")]
#[derive(Resource, Default)]
struct RestartSocialRequested(bool);

#[cfg(feature = "social")]
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/restart_social")]
struct RestartSocialCommand;

#[cfg(feature = "social")]
fn restart_social(
    mut input: ConsoleCommand<RestartSocialCommand>,
    mut restart: ResMut<RestartSocialRequested>,
) {
    if let Some(Ok(_)) = input.take() {
        restart.0 = true;
        input.reply_ok("social client restart requested");
    }
}

#[cfg(feature = "social")]
#[derive(Resource, Default)]
struct DebugSocialEnabled(bool);

#[cfg(feature = "social")]
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/debug_social")]
struct DebugSocialCommand {
    on: Option<bool>,
}

#[cfg(feature = "social")]
fn toggle_debug_social(
    mut input: ConsoleCommand<DebugSocialCommand>,
    mut enabled: ResMut<DebugSocialEnabled>,
    mut debug: ResMut<DebugInfo>,
) {
    if let Some(Ok(command)) = input.take() {
        enabled.0 = command.on.unwrap_or(!enabled.0);
        if !enabled.0 {
            debug.info.remove("Social client");
            debug.info.remove("Social friends");
            debug.info.remove("Social connectivity");
        }
        input.reply_ok(format!("social debug info: {}", enabled.0));
    }
}

#[cfg(feature = "social")]
fn debug_write_social(social: Res<SocialClient>, mut debug: ResMut<DebugInfo>) {
    let Some(client) = social.0.as_ref() else {
        debug
            .info
            .insert("Social client", "not connected".to_string());
        debug.info.remove("Social friends");
        debug.info.remove("Social connectivity");
        return;
    };
    debug.info.insert(
        "Social client",
        format!(
            "live={}, initialized={}",
            client.live(),
            client.is_initialized
        ),
    );
    debug.info.insert(
        "Social friends",
        format!(
            "friends: {}, sent: {}, received: {}",
            client.friends.len(),
            client.sent_requests.len(),
            client.received_requests.len(),
        ),
    );
    use dcl_component::proto_components::social_service::v2::ConnectivityStatus;
    let online = client
        .friend_status
        .values()
        .filter(|s| matches!(s, ConnectivityStatus::Online))
        .count();
    let away = client
        .friend_status
        .values()
        .filter(|s| matches!(s, ConnectivityStatus::Away))
        .count();
    debug.info.insert(
        "Social connectivity",
        format!(
            "online: {online}, away: {away}, tracked: {}",
            client.friend_status.len()
        ),
    );
}

#[cfg(feature = "social")]
#[allow(clippy::too_many_arguments)]
fn init_social_client(
    mut commands: Commands,
    wallet: Res<Wallet>,
    mut social: ResMut<SocialClient>,
    mut friends: Local<Option<UnboundedReceiver<FriendshipEvent>>>,
    mut connectivity: Local<Option<UnboundedReceiver<ConnectivityEvent>>>,
    mut chats: Local<Option<UnboundedReceiver<DirectChatEvent>>>,
    social_runtime: Res<runtime::SocialRuntime>,
    mut restart: ResMut<RestartSocialRequested>,
) {
    let restart_requested = std::mem::take(&mut restart.0);
    if (wallet.is_changed() || restart_requested) && wallet.address().is_some() {
        // Drop the old handler (and its event channels) before opening a new
        // connection so the old async task observes its channels closed and tears
        // down its WS, rather than overlapping with the new one.
        social.0 = None;
        *friends = None;
        *connectivity = None;
        *chats = None;

        let (f_sx, f_rx) = unbounded_channel();
        let (conn_sx, conn_rx) = unbounded_channel();
        let (c_sx, c_rx) = unbounded_channel();
        let client = SocialClientHandler::connect(
            wallet.clone(),
            &social_runtime,
            move |f| {
                let _ = f_sx.send(FriendshipEvent(Some(f.clone())));
            },
            move |address, status| {
                let _ = conn_sx.send(ConnectivityEvent {
                    address,
                    status: status as i32,
                });
            },
            move |c| {
                let _ = c_sx.send(DirectChatEvent(c));
            },
        );
        social.0 = client;
        *friends = Some(f_rx);
        *connectivity = Some(conn_rx);
        *chats = Some(c_rx);
    }

    drain_events(&mut commands, &mut friends, &mut connectivity, &mut chats);
}

#[cfg(not(feature = "social"))]
fn init_social_client(
    mut commands: Commands,
    wallet: Res<Wallet>,
    mut social: ResMut<SocialClient>,
    mut friends: Local<Option<UnboundedReceiver<FriendshipEvent>>>,
    mut connectivity: Local<Option<UnboundedReceiver<ConnectivityEvent>>>,
    mut chats: Local<Option<UnboundedReceiver<DirectChatEvent>>>,
) {
    if wallet.is_changed() && wallet.address().is_some() {
        let (f_sx, f_rx) = unbounded_channel();
        let (conn_sx, conn_rx) = unbounded_channel();
        let (c_sx, c_rx) = unbounded_channel();
        let client = SocialClientHandler::connect(
            wallet.clone(),
            move |f| {
                let _ = f_sx.send(FriendshipEvent(Some(f.clone())));
            },
            move |address, status| {
                let _ = conn_sx.send(ConnectivityEvent {
                    address,
                    status: status as i32,
                });
            },
            move |c| {
                let _ = c_sx.send(DirectChatEvent(c));
            },
        );
        social.0 = client;
        *friends = Some(f_rx);
        *connectivity = Some(conn_rx);
        *chats = Some(c_rx);
    }

    drain_events(&mut commands, &mut friends, &mut connectivity, &mut chats);
}

fn drain_events(
    commands: &mut Commands,
    friends: &mut Option<UnboundedReceiver<FriendshipEvent>>,
    connectivity: &mut Option<UnboundedReceiver<ConnectivityEvent>>,
    chats: &mut Option<UnboundedReceiver<DirectChatEvent>>,
) {
    while let Some(f) = friends.as_mut().and_then(|rx| rx.try_recv().ok()) {
        commands.send_event(f);
    }
    while let Some(ev) = connectivity.as_mut().and_then(|rx| rx.try_recv().ok()) {
        commands.send_event(ev);
    }
    while let Some(c) = chats.as_mut().and_then(|rx| rx.try_recv().ok()) {
        commands.send_event(c);
    }
}

#[derive(Resource, Default)]
pub struct SocialClient(pub Option<SocialClientHandler>);

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum FriendshipState {
    NotFriends,
    SentRequest,
    RecdRequested,
    Friends,
    Error,
}

impl SocialClient {
    pub fn get_state(&self, address: Address) -> FriendshipState {
        let Some(client) = self.0.as_ref() else {
            return FriendshipState::Error;
        };
        if client.friends.contains_key(&address) {
            return FriendshipState::Friends;
        }
        if client.sent_requests.contains_key(&address) {
            return FriendshipState::SentRequest;
        }
        if client.received_requests.contains_key(&address) {
            return FriendshipState::RecdRequested;
        }
        FriendshipState::NotFriends
    }
}

#[cfg(feature = "social")]
fn convert_name_color(
    color: &Option<dcl_component::proto_components::common::Color3>,
) -> Option<NameColor> {
    color.as_ref().map(|c| NameColor {
        r: c.r,
        g: c.g,
        b: c.b,
    })
}

/// Handles request/response SystemApi messages for friends
fn handle_social_requests(mut events: EventReader<SystemApi>, mut social: ResMut<SocialClient>) {
    for event in events.read() {
        match event {
            #[cfg(feature = "social")]
            SystemApi::GetFriends(sx) => {
                let friends: Vec<FriendData> = social
                    .0
                    .as_ref()
                    .map(|c| {
                        c.friends
                            .iter()
                            .map(|(a, profile)| FriendData {
                                address: format!("{a:#x}"),
                                name: profile.name.clone(),
                                has_claimed_name: profile.has_claimed_name,
                                profile_picture_url: profile.profile_picture_url.clone(),
                                name_color: convert_name_color(&profile.name_color),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                sx.send(friends);
            }
            #[cfg(not(feature = "social"))]
            SystemApi::GetFriends(sx) => {
                let friends: Vec<FriendData> = social
                    .0
                    .as_ref()
                    .map(|c| {
                        c.friends
                            .iter()
                            .map(|(a, profile)| FriendData {
                                address: format!("{a:#x}"),
                                name: profile.name.clone(),
                                has_claimed_name: profile.has_claimed_name,
                                profile_picture_url: profile.profile_picture_url.clone(),
                                name_color: None,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                sx.send(friends);
            }
            #[cfg(feature = "social")]
            SystemApi::GetSentFriendRequests(sx) => {
                let requests: Vec<FriendRequestData> = social
                    .0
                    .as_ref()
                    .map(|c| {
                        c.sent_requests
                            .iter()
                            .map(|(a, req)| {
                                let profile = req.friend.as_ref();
                                FriendRequestData {
                                    address: format!("{a:#x}"),
                                    name: profile.map(|p| p.name.clone()).unwrap_or_default(),
                                    has_claimed_name: profile
                                        .map(|p| p.has_claimed_name)
                                        .unwrap_or(false),
                                    profile_picture_url: profile
                                        .map(|p| p.profile_picture_url.clone())
                                        .unwrap_or_default(),
                                    name_color: profile
                                        .and_then(|p| convert_name_color(&p.name_color)),
                                    created_at: req.created_at,
                                    message: req.message.clone(),
                                    id: req.id.clone(),
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                sx.send(requests);
            }
            #[cfg(not(feature = "social"))]
            SystemApi::GetSentFriendRequests(sx) => {
                let requests: Vec<FriendRequestData> = social
                    .0
                    .as_ref()
                    .map(|c| {
                        c.sent_requests
                            .iter()
                            .map(|(a, req)| {
                                let profile = req.friend.as_ref();
                                FriendRequestData {
                                    address: format!("{a:#x}"),
                                    name: profile.map(|p| p.name.clone()).unwrap_or_default(),
                                    has_claimed_name: profile
                                        .map(|p| p.has_claimed_name)
                                        .unwrap_or(false),
                                    profile_picture_url: profile
                                        .map(|p| p.profile_picture_url.clone())
                                        .unwrap_or_default(),
                                    name_color: None,
                                    created_at: req.created_at,
                                    message: req.message.clone(),
                                    id: req.id.clone(),
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                sx.send(requests);
            }
            #[cfg(feature = "social")]
            SystemApi::GetReceivedFriendRequests(sx) => {
                let requests: Vec<FriendRequestData> = social
                    .0
                    .as_ref()
                    .map(|c| {
                        c.received_requests
                            .iter()
                            .map(|(a, req)| {
                                let profile = req.friend.as_ref();
                                FriendRequestData {
                                    address: format!("{a:#x}"),
                                    name: profile.map(|p| p.name.clone()).unwrap_or_default(),
                                    has_claimed_name: profile
                                        .map(|p| p.has_claimed_name)
                                        .unwrap_or(false),
                                    profile_picture_url: profile
                                        .map(|p| p.profile_picture_url.clone())
                                        .unwrap_or_default(),
                                    name_color: profile
                                        .and_then(|p| convert_name_color(&p.name_color)),
                                    created_at: req.created_at,
                                    message: req.message.clone(),
                                    id: req.id.clone(),
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                sx.send(requests);
            }
            #[cfg(not(feature = "social"))]
            SystemApi::GetReceivedFriendRequests(sx) => {
                let requests: Vec<FriendRequestData> = social
                    .0
                    .as_ref()
                    .map(|c| {
                        c.received_requests
                            .iter()
                            .map(|(a, req)| {
                                let profile = req.friend.as_ref();
                                FriendRequestData {
                                    address: format!("{a:#x}"),
                                    name: profile.map(|p| p.name.clone()).unwrap_or_default(),
                                    has_claimed_name: profile
                                        .map(|p| p.has_claimed_name)
                                        .unwrap_or(false),
                                    profile_picture_url: profile
                                        .map(|p| p.profile_picture_url.clone())
                                        .unwrap_or_default(),
                                    name_color: None,
                                    created_at: req.created_at,
                                    message: req.message.clone(),
                                    id: req.id.clone(),
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                sx.send(requests);
            }
            #[cfg(feature = "social")]
            SystemApi::GetMutualFriends(address, sx) => {
                let sx = sx.clone();
                match social
                    .0
                    .as_ref()
                    .and_then(|c| c.get_mutual_friends(address.clone()).ok())
                {
                    Some(rx) => {
                        IoTaskPool::get()
                            .spawn(async move {
                                let result = match rx.await {
                                    Ok(Ok(profiles)) => profiles
                                        .iter()
                                        .map(|profile| FriendData {
                                            address: profile.address.clone(),
                                            name: profile.name.clone(),
                                            has_claimed_name: profile.has_claimed_name,
                                            profile_picture_url: profile
                                                .profile_picture_url
                                                .clone(),
                                            name_color: convert_name_color(&profile.name_color),
                                        })
                                        .collect(),
                                    _ => Vec::new(),
                                };
                                sx.send(result);
                            })
                            .detach();
                    }
                    None => {
                        sx.send(Vec::new());
                    }
                }
            }
            #[cfg(not(feature = "social"))]
            SystemApi::GetMutualFriends(_, sx) => {
                sx.send(Vec::new());
            }
            SystemApi::GetSocialInitialized(sx) => {
                let initialized = social.0.as_ref().map(|c| c.is_initialized).unwrap_or(false);
                sx.send(initialized);
            }
            SystemApi::SendFriendRequest(address, message, sx) => {
                let result = (|| {
                    let addr = address.as_h160().ok_or("invalid address")?;
                    let client = social.0.as_mut().ok_or("social not initialized")?;
                    client
                        .friend_request(addr, message.clone())
                        .map_err(|e| format!("{e}"))
                })();
                sx.send(result.map_err(|e| e.to_string()));
            }
            SystemApi::AcceptFriendRequest(address, sx) => {
                let result = (|| {
                    let addr = address.as_h160().ok_or("invalid address")?;
                    let client = social.0.as_mut().ok_or("social not initialized")?;
                    client.accept_request(addr).map_err(|e| format!("{e}"))
                })();
                sx.send(result.map_err(|e| e.to_string()));
            }
            SystemApi::RejectFriendRequest(address, sx) => {
                let result = (|| {
                    let addr = address.as_h160().ok_or("invalid address")?;
                    let client = social.0.as_mut().ok_or("social not initialized")?;
                    client.reject_request(addr).map_err(|e| format!("{e}"))
                })();
                sx.send(result.map_err(|e| e.to_string()));
            }
            SystemApi::CancelFriendRequest(address, sx) => {
                let result = (|| {
                    let addr = address.as_h160().ok_or("invalid address")?;
                    let client = social.0.as_mut().ok_or("social not initialized")?;
                    client.cancel_request(addr).map_err(|e| format!("{e}"))
                })();
                sx.send(result.map_err(|e| e.to_string()));
            }
            SystemApi::DeleteFriend(address, sx) => {
                let result = (|| {
                    let addr = address.as_h160().ok_or("invalid address")?;
                    let client = social.0.as_mut().ok_or("social not initialized")?;
                    client.delete_friend(addr).map_err(|e| format!("{e}"))
                })();
                sx.send(result.map_err(|e| e.to_string()));
            }
            #[cfg(feature = "social")]
            SystemApi::GetOnlineFriends(sx) => {
                use dcl_component::proto_components::social_service::v2::ConnectivityStatus;
                let data: Vec<FriendStatusData> = social
                    .0
                    .as_ref()
                    .map(|c| {
                        c.friends
                            .iter()
                            .map(|(a, profile)| {
                                let status = c
                                    .friend_status
                                    .get(a)
                                    .copied()
                                    .unwrap_or(ConnectivityStatus::Offline);
                                FriendStatusData {
                                    address: format!("{a:#x}"),
                                    name: profile.name.clone(),
                                    has_claimed_name: profile.has_claimed_name,
                                    profile_picture_url: profile.profile_picture_url.clone(),
                                    name_color: convert_name_color(&profile.name_color),
                                    status: match status {
                                        ConnectivityStatus::Online => "online".to_owned(),
                                        ConnectivityStatus::Offline => "offline".to_owned(),
                                        ConnectivityStatus::Away => "away".to_owned(),
                                    },
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                sx.send(data);
            }
            #[cfg(not(feature = "social"))]
            SystemApi::GetOnlineFriends(sx) => {
                let data: Vec<FriendStatusData> = social
                    .0
                    .as_ref()
                    .map(|c| {
                        c.friends
                            .iter()
                            .map(|(a, profile)| FriendStatusData {
                                address: format!("{a:#x}"),
                                name: profile.name.clone(),
                                has_claimed_name: profile.has_claimed_name,
                                profile_picture_url: profile.profile_picture_url.clone(),
                                name_color: None,
                                status: "offline".to_owned(),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                sx.send(data);
            }
            #[cfg(feature = "social")]
            SystemApi::BlockUser(address, sx) => {
                let sx = sx.clone();
                match social
                    .0
                    .as_ref()
                    .and_then(|c| c.block_user(address.clone()).ok())
                {
                    Some(rx) => {
                        IoTaskPool::get()
                            .spawn(async move {
                                let result = match rx.await {
                                    Ok(r) => r,
                                    Err(_) => Err("channel closed".to_string()),
                                };
                                sx.send(result);
                            })
                            .detach();
                    }
                    None => {
                        sx.send(Err("social not initialized".to_string()));
                    }
                }
            }
            #[cfg(not(feature = "social"))]
            SystemApi::BlockUser(_, sx) => {
                sx.send(Err("social not available".to_string()));
            }
            #[cfg(feature = "social")]
            SystemApi::UnblockUser(address, sx) => {
                let sx = sx.clone();
                match social
                    .0
                    .as_ref()
                    .and_then(|c| c.unblock_user(address.clone()).ok())
                {
                    Some(rx) => {
                        IoTaskPool::get()
                            .spawn(async move {
                                let result = match rx.await {
                                    Ok(r) => r,
                                    Err(_) => Err("channel closed".to_string()),
                                };
                                sx.send(result);
                            })
                            .detach();
                    }
                    None => {
                        sx.send(Err("social not initialized".to_string()));
                    }
                }
            }
            #[cfg(not(feature = "social"))]
            SystemApi::UnblockUser(_, sx) => {
                sx.send(Err("social not available".to_string()));
            }
            #[cfg(feature = "social")]
            SystemApi::GetBlockedUsers(sx) => {
                let sx = sx.clone();
                match social.0.as_ref().and_then(|c| c.get_blocked_users().ok()) {
                    Some(rx) => {
                        IoTaskPool::get()
                            .spawn(async move {
                                let result = match rx.await {
                                    Ok(Ok(profiles)) => profiles
                                        .iter()
                                        .map(|profile| BlockedUserData {
                                            address: profile.address.clone(),
                                            name: profile.name.clone(),
                                            has_claimed_name: profile.has_claimed_name,
                                            profile_picture_url: profile
                                                .profile_picture_url
                                                .clone(),
                                            name_color: convert_name_color(&profile.name_color),
                                        })
                                        .collect(),
                                    _ => Vec::new(),
                                };
                                sx.send(result);
                            })
                            .detach();
                    }
                    None => {
                        sx.send(Vec::new());
                    }
                }
            }
            #[cfg(not(feature = "social"))]
            SystemApi::GetBlockedUsers(sx) => {
                sx.send(Vec::new());
            }
            _ => {}
        }
    }
}

/// Pipes FriendshipEvent bevy events to scene stream subscribers
fn pipe_friendship_events_to_scene(
    mut requests: EventReader<SystemApi>,
    mut friendship_events: EventReader<FriendshipEvent>,
    mut senders: Local<Vec<RpcStreamSender<FriendshipEventUpdate>>>,
) {
    senders.extend(requests.read().filter_map(|ev| {
        if let SystemApi::GetFriendshipEventStream(sender) = ev {
            Some(sender.clone())
        } else {
            None
        }
    }));
    senders.retain(|s| !s.is_closed());

    for ev in friendship_events.read() {
        if let Some(update) = friendship_event_to_update(&ev.0) {
            for sender in senders.iter() {
                let _ = sender.send(update.clone());
            }
        }
    }
}

/// Pipes ConnectivityEvent bevy events to scene stream subscribers
fn pipe_connectivity_events_to_scene(
    mut requests: EventReader<SystemApi>,
    mut connectivity_events: EventReader<ConnectivityEvent>,
    mut senders: Local<Vec<RpcStreamSender<FriendConnectivityEvent>>>,
    social: Res<SocialClient>,
) {
    senders.extend(requests.read().filter_map(|ev| {
        if let SystemApi::GetFriendConnectivityStream(sender) = ev {
            Some(sender.clone())
        } else {
            None
        }
    }));
    senders.retain(|s| !s.is_closed());

    if senders.is_empty() {
        connectivity_events.clear();
        return;
    }

    for ev in connectivity_events.read() {
        let status = match ev.status {
            0 => "online",
            2 => "away",
            _ => "offline",
        };

        // Look up the friend profile for full data
        let Some(client) = social.0.as_ref() else {
            continue;
        };
        let event = if let Some(profile) = client.friends.get(&ev.address) {
            #[cfg(feature = "social")]
            let name_color = convert_name_color(&profile.name_color);
            #[cfg(not(feature = "social"))]
            let name_color = None;

            FriendConnectivityEvent {
                address: format!("{:#x}", ev.address),
                name: profile.name.clone(),
                has_claimed_name: profile.has_claimed_name,
                profile_picture_url: profile.profile_picture_url.clone(),
                name_color,
                status: status.to_owned(),
            }
        } else {
            FriendConnectivityEvent {
                address: format!("{:#x}", ev.address),
                name: String::new(),
                has_claimed_name: false,
                profile_picture_url: String::new(),
                name_color: None,
                status: status.to_owned(),
            }
        };

        for sender in senders.iter() {
            let _ = sender.send(event.clone());
        }
    }
}

fn friendship_event_to_update(body: &Option<FriendshipEventBody>) -> Option<FriendshipEventUpdate> {
    #[cfg(feature = "social")]
    {
        use dcl_component::proto_components::social_service::v2::friendship_update;
        match body.as_ref()? {
            friendship_update::Update::Request(r) => {
                let profile = r.friend.as_ref()?;
                let addr = profile.address.as_h160()?;
                Some(FriendshipEventUpdate::Request {
                    address: format!("{addr:#x}"),
                    name: profile.name.clone(),
                    has_claimed_name: profile.has_claimed_name,
                    profile_picture_url: profile.profile_picture_url.clone(),
                    name_color: convert_name_color(&profile.name_color),
                    created_at: r.created_at,
                    message: r.message.clone(),
                    id: r.id.clone(),
                })
            }
            friendship_update::Update::Accept(r) => {
                let addr = r.user.as_ref()?.address.as_h160()?;
                Some(FriendshipEventUpdate::Accept {
                    address: format!("{addr:#x}"),
                })
            }
            friendship_update::Update::Reject(r) => {
                let addr = r.user.as_ref()?.address.as_h160()?;
                Some(FriendshipEventUpdate::Reject {
                    address: format!("{addr:#x}"),
                })
            }
            friendship_update::Update::Delete(r) => {
                let addr = r.user.as_ref()?.address.as_h160()?;
                Some(FriendshipEventUpdate::Delete {
                    address: format!("{addr:#x}"),
                })
            }
            friendship_update::Update::Cancel(r) => {
                let addr = r.user.as_ref()?.address.as_h160()?;
                Some(FriendshipEventUpdate::Cancel {
                    address: format!("{addr:#x}"),
                })
            }
            friendship_update::Update::Block(r) => {
                let addr = r.user.as_ref()?.address.as_h160()?;
                Some(FriendshipEventUpdate::Block {
                    address: format!("{addr:#x}"),
                })
            }
        }
    }
    #[cfg(not(feature = "social"))]
    {
        match body.as_ref()? {
            FriendshipEventBody::Request(r) => {
                let addr = &r.friend.as_ref()?.address;
                Some(FriendshipEventUpdate::Request {
                    address: addr.clone(),
                    name: String::new(),
                    has_claimed_name: false,
                    profile_picture_url: String::new(),
                    name_color: None,
                    created_at: 0,
                    message: None,
                    id: String::new(),
                })
            }
            FriendshipEventBody::Accept(r) => {
                let addr = &r.user.as_ref()?.address;
                Some(FriendshipEventUpdate::Accept {
                    address: addr.clone(),
                })
            }
            FriendshipEventBody::Reject(r) => {
                let addr = &r.user.as_ref()?.address;
                Some(FriendshipEventUpdate::Reject {
                    address: addr.clone(),
                })
            }
            FriendshipEventBody::Delete(r) => {
                let addr = &r.user.as_ref()?.address;
                Some(FriendshipEventUpdate::Delete {
                    address: addr.clone(),
                })
            }
            FriendshipEventBody::Cancel(r) => {
                let addr = &r.user.as_ref()?.address;
                Some(FriendshipEventUpdate::Cancel {
                    address: addr.clone(),
                })
            }
            FriendshipEventBody::Block(r) => {
                let addr = &r.user.as_ref()?.address;
                Some(FriendshipEventUpdate::Block {
                    address: addr.clone(),
                })
            }
        }
    }
}

#[derive(Event)]
pub struct FriendshipEvent(pub Option<FriendshipEventBody>);

#[derive(Event, Clone)]
pub struct ConnectivityEvent {
    pub address: Address,
    /// 0 = Online, 1 = Offline, 2 = Away
    pub status: i32,
}

#[derive(Event)]
pub struct DirectChatEvent(pub DirectChatMessage);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectChatMessage {
    pub partner: Address,
    pub me_speaking: bool,
    pub message: String,
}
