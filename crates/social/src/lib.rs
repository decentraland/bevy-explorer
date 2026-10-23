#[cfg(not(feature = "social"))]
mod fake_client;
use alloy_core::primitives::Address;
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
use bevy::tasks::IoTaskPool;
#[cfg(feature = "social")]
use bevy_console::ConsoleCommand;
use common::rpc::{RpcResultSender, RpcStreamSender};
#[cfg(feature = "social")]
use common::structs::DebugInfo;
use common::util::AsH160;
#[cfg(feature = "social")]
use console::DoAddConsoleCommand;
use notifications::PushNotification;
#[cfg(feature = "social")]
use system_bridge::BlockedUserData;
#[cfg(feature = "social")]
use system_bridge::BlockingStatusData;
#[cfg(feature = "social")]
use system_bridge::NameColor;
use system_bridge::{
    BlockUpdateData, FriendConnectivityEvent, FriendData, FriendRequestData, FriendStatusData,
    FriendshipEventUpdate, SystemApi,
};
use tokio::sync::{
    mpsc::{unbounded_channel, UnboundedReceiver},
    oneshot,
};
use wallet::Wallet;

pub struct SocialPlugin;

impl Plugin for SocialPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        #[cfg(feature = "social")]
        app.add_plugins(runtime::SocialRuntimePlugin);
        app.add_event::<FriendshipEvent>();
        app.add_event::<ConnectivityEvent>();
        app.add_event::<BlockUpdateEvent>();
        app.add_event::<DirectChatEvent>();
        app.init_resource::<SocialClient>();
        app.init_resource::<SocialConsumerRequested>();
        app.add_event::<SocialStateChanged>();
        app.add_systems(
            PostUpdate,
            |mut client: ResMut<SocialClient>, mut changed: EventWriter<SocialStateChanged>| {
                if client.0.as_mut().is_some_and(|client| client.update()) {
                    changed.write(SocialStateChanged);
                }
            },
        );
        app.add_systems(PostUpdate, init_social_client);
        app.add_systems(
            PostUpdate,
            (
                handle_social_requests,
                pipe_friendship_events_to_scene,
                pipe_connectivity_events_to_scene,
                pipe_block_updates_to_scene,
                // not in the social dev binary, which has no config or settings
                sync_dm_privacy
                    .run_if(resource_exists::<AppConfig>.and(resource_exists::<Settings>)),
            ),
        );
        #[cfg(feature = "social")]
        {
            app.init_resource::<DebugSocialEnabled>();
            app.init_resource::<RestartSocialRequested>();
            app.add_preview_console_command::<DebugSocialCommand, _>(toggle_debug_social);
            app.add_console_command::<RestartSocialCommand, _>(restart_social);
            app.add_console_command::<DmPrivacyCommand, _>(dm_privacy);
            app.add_console_command::<DmPrivacyOfCommand, _>(dm_privacy_of);
            app.add_systems(
                PostUpdate,
                debug_write_social.run_if(|e: Res<DebugSocialEnabled>| e.0),
            );
        }
    }
}

/// Sticky flag: flipped to true the first time a scene asks for anything social
/// via [`SystemApi`]. Gates [`init_social_client`] so the social client is not
/// connected for scenes that never use the friends API.
#[derive(Resource, Default)]
struct SocialConsumerRequested(bool);

fn is_social_consumer_request(event: &SystemApi) -> bool {
    matches!(
        event,
        SystemApi::GetFriends(_)
            | SystemApi::GetSentFriendRequests(_)
            | SystemApi::GetReceivedFriendRequests(_)
            | SystemApi::GetSocialInitialized(_)
            | SystemApi::SendFriendRequest(_, _, _)
            | SystemApi::AcceptFriendRequest(_, _)
            | SystemApi::RejectFriendRequest(_, _)
            | SystemApi::CancelFriendRequest(_, _)
            | SystemApi::DeleteFriend(_, _)
            | SystemApi::GetFriendshipEventStream(_)
            | SystemApi::GetMutualFriends(_, _)
            | SystemApi::GetOnlineFriends(_)
            | SystemApi::GetFriendConnectivityStream(_)
            | SystemApi::BlockUser(_, _)
            | SystemApi::UnblockUser(_, _)
            | SystemApi::GetBlockedUsers(_)
            | SystemApi::GetBlockingStatus(_)
            | SystemApi::GetBlockUpdateStream(_)
            | SystemApi::GetDmUserStateStream(_, _)
    )
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

use common::structs::AppConfig;
pub use common::structs::{DmPrivacy, DmPrivacyOf};
use system_bridge::settings::Settings;

/// Keeps the DM privacy setting and the social service in step. The config slot is not
/// persisted. When the client initializes, an unset slot takes the server value and a slot the
/// user already set is upserted; from then on a slot that differs from the server value is a
/// user change to upsert. A change is requested once and left pending until the server echoes
/// it; a rejected change reverts the slot to the server value. The slot is cleared again when
/// the client goes away, so the next account does not inherit it. Engine-driven writes bypass
/// change detection so they refresh the settings panel without re-applying every setting.
fn sync_dm_privacy(
    mut config: ResMut<AppConfig>,
    mut settings: ResMut<Settings>,
    social: Res<SocialClient>,
    mut synced_from_server: Local<bool>,
    mut requested: Local<Option<DmPrivacy>>,
    mut in_flight: Local<Option<oneshot::Receiver<Result<DmPrivacy, String>>>>,
) {
    let Some(client) = social.0.as_ref().filter(|c| c.is_initialized) else {
        if *synced_from_server {
            config.bypass_change_detection().dm_privacy = None;
            settings.refresh(&config);
            *synced_from_server = false;
        }
        *requested = None;
        *in_flight = None;
        return;
    };
    if !*synced_from_server {
        *synced_from_server = true;
        *requested = None;
        *in_flight = None;
        if config.dm_privacy.is_none() {
            config.bypass_change_detection().dm_privacy = Some(client.dm_privacy);
            settings.refresh(&config);
            return;
        }
    }
    if let Some(rx) = in_flight.as_mut() {
        let outcome = match rx.try_recv() {
            Err(oneshot::error::TryRecvError::Empty) => None,
            Ok(Ok(_)) => Some(Ok(())),
            Ok(Err(e)) => Some(Err(e)),
            Err(oneshot::error::TryRecvError::Closed) => Some(Err("client closed".to_owned())),
        };
        match outcome {
            None => (),
            Some(Ok(())) => *in_flight = None,
            Some(Err(e)) => {
                warn!("[social] dm privacy change rejected: {e}");
                config.bypass_change_detection().dm_privacy = Some(client.dm_privacy);
                settings.refresh(&config);
                *requested = None;
                *in_flight = None;
            }
        }
    }
    let Some(wanted) = config.dm_privacy else {
        return;
    };
    if wanted == client.dm_privacy {
        *requested = None;
        return;
    }
    if *requested == Some(wanted) {
        return;
    }
    match client.upsert_social_settings(wanted) {
        Ok(rx) => {
            *requested = Some(wanted);
            *in_flight = Some(rx);
        }
        Err(e) => warn!("[social] failed to request dm privacy change: {e}"),
    }
}

/// `/dm_privacy` prints the local user's DM privacy; `/dm_privacy all|friends` sets it through
/// the setting, so the change goes to the server the same way the settings panel's does.
#[cfg(feature = "social")]
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/dm_privacy")]
struct DmPrivacyCommand {
    /// `all` or `friends`
    privacy: Option<String>,
}

#[cfg(feature = "social")]
fn dm_privacy(
    mut input: ConsoleCommand<DmPrivacyCommand>,
    social: Res<SocialClient>,
    mut config: ResMut<AppConfig>,
    mut pending: ResMut<console::PendingConsoleResponses>,
) {
    let Some(Ok(command)) = input.take() else {
        return;
    };
    let Some(client) = social.0.as_ref() else {
        input.reply_failed("social not initialized");
        return;
    };
    let privacy = match command.privacy.as_deref() {
        None => None,
        Some("all") => Some(DmPrivacy::All),
        Some("friends") => Some(DmPrivacy::OnlyFriends),
        Some(other) => {
            input.reply_failed(format!("unknown privacy `{other}`: use `all` or `friends`"));
            return;
        }
    };
    if let Some(privacy) = privacy {
        config.dm_privacy = Some(privacy);
        input.reply_ok(format!("dm privacy requested: {privacy:?}"));
        return;
    }
    let rx = match client.get_social_settings() {
        Ok(rx) => rx,
        Err(e) => {
            input.reply_failed(format!("{e}"));
            return;
        }
    };
    let responder = input.take_responder();
    pending.push_oneshot(
        rx,
        |result| result.map(|privacy| format!("dm privacy: {privacy:?}")),
        responder,
    );
}

/// `/dm_privacy_of <address>...` prints other users' DM privacy and whether they are friends.
#[cfg(feature = "social")]
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/dm_privacy_of")]
struct DmPrivacyOfCommand {
    #[arg(required = true)]
    addresses: Vec<String>,
}

#[cfg(feature = "social")]
fn dm_privacy_of(
    mut input: ConsoleCommand<DmPrivacyOfCommand>,
    social: Res<SocialClient>,
    mut pending: ResMut<console::PendingConsoleResponses>,
) {
    let Some(Ok(command)) = input.take() else {
        return;
    };
    let Some(client) = social.0.as_ref() else {
        input.reply_failed("social not initialized");
        return;
    };
    let rx = match client.get_private_messages_settings(command.addresses) {
        Ok(rx) => rx,
        Err(e) => {
            input.reply_failed(format!("{e}"));
            return;
        }
    };
    let responder = input.take_responder();
    pending.push_oneshot(
        rx,
        |result| {
            result.map(|entries| {
                entries
                    .iter()
                    .map(|e| format!("{}: {:?}", e.address, e.privacy))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
        },
        responder,
    );
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
    mut block_updates: Local<Option<UnboundedReceiver<BlockUpdateEvent>>>,
    mut chats: Local<Option<UnboundedReceiver<DirectChatEvent>>>,
    social_runtime: runtime::SocialRuntimeRes,
    mut restart: ResMut<RestartSocialRequested>,
    mut consumer_requested: ResMut<SocialConsumerRequested>,
    mut system_api_events: EventReader<SystemApi>,
) {
    for event in system_api_events.read() {
        if is_social_consumer_request(event) {
            consumer_requested.0 = true;
        }
    }

    // logged out: drop the connection, or the previous account stays online and its
    // events keep arriving until the next login replaces it
    if wallet.is_changed() && wallet.address().is_none() {
        social.0 = None;
        *friends = None;
        *connectivity = None;
        *block_updates = None;
        *chats = None;
    }

    let restart_requested = std::mem::take(&mut restart.0);
    let reconnect_signal = wallet.is_changed() || restart_requested || social.0.is_none();
    if reconnect_signal && consumer_requested.0 && wallet.address().is_some() {
        // Drop the old handler (and its event channels) before opening a new
        // connection so the old async task observes its channels closed and tears
        // down its WS, rather than overlapping with the new one.
        social.0 = None;
        *friends = None;
        *connectivity = None;
        *block_updates = None;
        *chats = None;

        let (f_sx, f_rx) = unbounded_channel();
        let (conn_sx, conn_rx) = unbounded_channel();
        let (block_sx, block_rx) = unbounded_channel();
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
            move |address, is_blocked| {
                let _ = block_sx.send(BlockUpdateEvent {
                    address: address.to_owned(),
                    is_blocked,
                });
            },
            move |c| {
                let _ = c_sx.send(DirectChatEvent(c));
            },
        );
        social.0 = client;
        *friends = Some(f_rx);
        *connectivity = Some(conn_rx);
        *block_updates = Some(block_rx);
        *chats = Some(c_rx);
    }

    drain_events(
        &mut commands,
        &mut friends,
        &mut connectivity,
        &mut block_updates,
        &mut chats,
    );
}

#[cfg(not(feature = "social"))]
#[allow(clippy::too_many_arguments)]
fn init_social_client(
    mut commands: Commands,
    wallet: Res<Wallet>,
    mut social: ResMut<SocialClient>,
    mut friends: Local<Option<UnboundedReceiver<FriendshipEvent>>>,
    mut connectivity: Local<Option<UnboundedReceiver<ConnectivityEvent>>>,
    mut block_updates: Local<Option<UnboundedReceiver<BlockUpdateEvent>>>,
    mut chats: Local<Option<UnboundedReceiver<DirectChatEvent>>>,
    mut consumer_requested: ResMut<SocialConsumerRequested>,
    mut system_api_events: EventReader<SystemApi>,
) {
    for event in system_api_events.read() {
        if is_social_consumer_request(event) {
            consumer_requested.0 = true;
        }
    }

    // logged out: drop the connection, or the previous account stays online and its
    // events keep arriving until the next login replaces it
    if wallet.is_changed() && wallet.address().is_none() {
        social.0 = None;
        *friends = None;
        *connectivity = None;
        *block_updates = None;
        *chats = None;
    }

    let reconnect_signal = wallet.is_changed() || social.0.is_none();
    if reconnect_signal && consumer_requested.0 && wallet.address().is_some() {
        let (f_sx, f_rx) = unbounded_channel();
        let (conn_sx, conn_rx) = unbounded_channel();
        let (block_sx, block_rx) = unbounded_channel();
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
            move |address, is_blocked| {
                let _ = block_sx.send(BlockUpdateEvent {
                    address: address.to_owned(),
                    is_blocked,
                });
            },
            move |c| {
                let _ = c_sx.send(DirectChatEvent(c));
            },
        );
        social.0 = client;
        *friends = Some(f_rx);
        *connectivity = Some(conn_rx);
        *block_updates = Some(block_rx);
        *chats = Some(c_rx);
    }

    drain_events(
        &mut commands,
        &mut friends,
        &mut connectivity,
        &mut block_updates,
        &mut chats,
    );
}

fn drain_events(
    commands: &mut Commands,
    friends: &mut Option<UnboundedReceiver<FriendshipEvent>>,
    connectivity: &mut Option<UnboundedReceiver<ConnectivityEvent>>,
    block_updates: &mut Option<UnboundedReceiver<BlockUpdateEvent>>,
    chats: &mut Option<UnboundedReceiver<DirectChatEvent>>,
) {
    while let Some(f) = friends.as_mut().and_then(|rx| rx.try_recv().ok()) {
        commands.send_event(f);
    }
    while let Some(ev) = connectivity.as_mut().and_then(|rx| rx.try_recv().ok()) {
        commands.send_event(ev);
    }
    while let Some(b) = block_updates.as_mut().and_then(|rx| rx.try_recv().ok()) {
        commands.send_event(b);
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

/// Submit a friendship action (request / accept / reject / cancel / delete)
/// and forward the backend's response back through `sx`. Synchronous failures
/// from the client wrapper (invalid address, "no request" preconditions,
/// client not initialized) reply immediately; otherwise we spawn a task to
/// await the dispatcher's reply so the caller sees the actual RPC outcome
/// (e.g. `BlockedUserError`) instead of a silent success.
fn spawn_friendship_action(
    sx: RpcResultSender<Result<(), String>>,
    client: Option<&mut SocialClientHandler>,
    submit: impl FnOnce(
        &mut SocialClientHandler,
    ) -> Result<tokio::sync::oneshot::Receiver<Result<(), String>>, String>,
) {
    let outcome = match client {
        Some(c) => submit(c),
        None => Err("social not initialized".to_string()),
    };
    match outcome {
        Ok(reply_rx) => {
            IoTaskPool::get()
                .spawn(async move {
                    let result = match reply_rx.await {
                        Ok(r) => r,
                        Err(_) => Err("channel closed before reply".to_string()),
                    };
                    sx.send(result);
                })
                .detach();
        }
        Err(e) => sx.send(Err(e)),
    }
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
                spawn_friendship_action(sx.clone(), social.0.as_mut(), |client| {
                    let addr = address.as_h160().ok_or("invalid address".to_string())?;
                    client
                        .friend_request(addr, message.clone())
                        .map_err(|e| e.to_string())
                });
            }
            SystemApi::AcceptFriendRequest(address, sx) => {
                spawn_friendship_action(sx.clone(), social.0.as_mut(), |client| {
                    let addr = address.as_h160().ok_or("invalid address".to_string())?;
                    client.accept_request(addr).map_err(|e| e.to_string())
                });
            }
            SystemApi::RejectFriendRequest(address, sx) => {
                spawn_friendship_action(sx.clone(), social.0.as_mut(), |client| {
                    let addr = address.as_h160().ok_or("invalid address".to_string())?;
                    client.reject_request(addr).map_err(|e| e.to_string())
                });
            }
            SystemApi::CancelFriendRequest(address, sx) => {
                spawn_friendship_action(sx.clone(), social.0.as_mut(), |client| {
                    let addr = address.as_h160().ok_or("invalid address".to_string())?;
                    client.cancel_request(addr).map_err(|e| e.to_string())
                });
            }
            SystemApi::DeleteFriend(address, sx) => {
                spawn_friendship_action(sx.clone(), social.0.as_mut(), |client| {
                    let addr = address.as_h160().ok_or("invalid address".to_string())?;
                    client.delete_friend(addr).map_err(|e| e.to_string())
                });
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
            #[cfg(feature = "social")]
            SystemApi::GetBlockingStatus(sx) => {
                let sx = sx.clone();
                match social.0.as_ref().and_then(|c| c.get_blocking_status().ok()) {
                    Some(rx) => {
                        IoTaskPool::get()
                            .spawn(async move {
                                let result = match rx.await {
                                    Ok(Ok((blocked_users, blocked_by_users))) => {
                                        Ok(BlockingStatusData {
                                            blocked_users,
                                            blocked_by_users,
                                        })
                                    }
                                    Ok(Err(e)) => Err(e),
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
            SystemApi::GetBlockingStatus(sx) => {
                sx.send(Err("social not available".to_string()));
            }
            _ => {}
        }
    }
}

/// Pipes BlockUpdateEvent bevy events to scene stream subscribers
fn pipe_block_updates_to_scene(
    mut requests: EventReader<SystemApi>,
    mut block_events: EventReader<BlockUpdateEvent>,
    mut senders: Local<Vec<RpcStreamSender<BlockUpdateData>>>,
) {
    senders.extend(requests.read().filter_map(|ev| {
        if let SystemApi::GetBlockUpdateStream(sender) = ev {
            Some(sender.clone())
        } else {
            None
        }
    }));
    senders.retain(|s| !s.is_closed());

    for ev in block_events.read() {
        let update = BlockUpdateData {
            address: ev.address.clone(),
            is_blocked: ev.is_blocked,
        };
        for sender in senders.iter() {
            let _ = sender.send(update.clone());
        }
    }
}

/// Pipes FriendshipEvent bevy events to scene stream subscribers
fn pipe_friendship_events_to_scene(
    mut requests: EventReader<SystemApi>,
    mut friendship_events: EventReader<FriendshipEvent>,
    mut push_notifications: EventWriter<PushNotification>,
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
        if let Some((update, maybe_notification)) =
            friendship_event_to_update_and_notification(&ev.0)
        {
            for sender in senders.iter() {
                let _ = sender.send(update.clone());
            }
            if let Some(notification) = maybe_notification {
                push_notifications.write(notification);
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

fn friendship_event_to_update_and_notification(
    body: &Option<FriendshipEventBody>,
) -> Option<(FriendshipEventUpdate, Option<PushNotification>)> {
    #[cfg(feature = "social")]
    {
        use dcl_component::proto_components::social_service::v2::friendship_update;
        match body.as_ref()? {
            friendship_update::Update::Request(r) => {
                let profile = r.friend.as_ref()?;
                let addr = profile.address.as_h160()?;
                Some((
                    FriendshipEventUpdate::Request {
                        address: format!("{addr:#x}"),
                        name: profile.name.clone(),
                        has_claimed_name: profile.has_claimed_name,
                        profile_picture_url: profile.profile_picture_url.clone(),
                        name_color: convert_name_color(&profile.name_color),
                        created_at: r.created_at,
                        message: r.message.clone(),
                        id: r.id.clone(),
                    },
                    Some(PushNotification {
                        title: "Friend Request".into(),
                        icon: Some(profile.profile_picture_url.clone()),
                        body: Some(format!("{} wants to be friends with you!", profile.name)),
                        timeout: 10.,
                    }),
                ))
            }
            friendship_update::Update::Accept(r) => {
                let user = r.user.as_ref()?;
                let addr = &user.address.as_h160()?;
                Some((
                    FriendshipEventUpdate::Accept {
                        address: format!("{addr:#x}"),
                    },
                    Some(PushNotification {
                        title: "New friend".into(),
                        icon: None,
                        body: Some("You have a new friend!".to_string()),
                        timeout: 10.,
                    }),
                ))
            }
            friendship_update::Update::Reject(r) => {
                let addr = r.user.as_ref()?.address.as_h160()?;
                Some((
                    FriendshipEventUpdate::Reject {
                        address: format!("{addr:#x}"),
                    },
                    None,
                ))
            }
            friendship_update::Update::Delete(r) => {
                let addr = r.user.as_ref()?.address.as_h160()?;
                Some((
                    FriendshipEventUpdate::Delete {
                        address: format!("{addr:#x}"),
                    },
                    None,
                ))
            }
            friendship_update::Update::Cancel(r) => {
                let addr = r.user.as_ref()?.address.as_h160()?;
                Some((
                    FriendshipEventUpdate::Cancel {
                        address: format!("{addr:#x}"),
                    },
                    None,
                ))
            }
            friendship_update::Update::Block(r) => {
                let addr = r.user.as_ref()?.address.as_h160()?;
                Some((
                    FriendshipEventUpdate::Block {
                        address: format!("{addr:#x}"),
                    },
                    None,
                ))
            }
        }
    }
    #[cfg(not(feature = "social"))]
    {
        match body.as_ref()? {
            FriendshipEventBody::Request(r) => {
                let profile = r.friend.as_ref()?;
                let addr = &profile.address;
                Some((
                    FriendshipEventUpdate::Request {
                        address: addr.clone(),
                        name: String::new(),
                        has_claimed_name: false,
                        profile_picture_url: String::new(),
                        name_color: None,
                        created_at: 0,
                        message: None,
                        id: String::new(),
                    },
                    Some(PushNotification {
                        title: "Friend Request".into(),
                        icon: None,
                        body: Some("Someone wants to be friends with you!".to_string()),
                        timeout: 10.,
                    }),
                ))
            }
            FriendshipEventBody::Accept(r) => {
                let addr = &r.user.as_ref()?.address;
                Some((
                    FriendshipEventUpdate::Accept {
                        address: addr.clone(),
                    },
                    None,
                ))
            }
            FriendshipEventBody::Reject(r) => {
                let addr = &r.user.as_ref()?.address;
                Some((
                    FriendshipEventUpdate::Reject {
                        address: addr.clone(),
                    },
                    None,
                ))
            }
            FriendshipEventBody::Delete(r) => {
                let addr = &r.user.as_ref()?.address;
                Some((
                    FriendshipEventUpdate::Delete {
                        address: addr.clone(),
                    },
                    None,
                ))
            }
            FriendshipEventBody::Cancel(r) => {
                let addr = &r.user.as_ref()?.address;
                Some((
                    FriendshipEventUpdate::Cancel {
                        address: addr.clone(),
                    },
                    None,
                ))
            }
            FriendshipEventBody::Block(r) => {
                let addr = &r.user.as_ref()?.address;
                Some((
                    FriendshipEventUpdate::Block {
                        address: addr.clone(),
                    },
                    None,
                ))
            }
        }
    }
}

#[derive(Event)]
pub struct FriendshipEvent(pub Option<FriendshipEventBody>);

/// The social client applied an update: friends, requests, connectivity, blocks or own settings.
#[derive(Event)]
pub struct SocialStateChanged;

#[derive(Event, Clone)]
pub struct ConnectivityEvent {
    pub address: Address,
    /// 0 = Online, 1 = Offline, 2 = Away
    pub status: i32,
}

/// Someone blocked / unblocked the local user.
#[derive(Event, Clone)]
pub struct BlockUpdateEvent {
    pub address: String,
    pub is_blocked: bool,
}

#[derive(Event)]
pub struct DirectChatEvent(pub DirectChatMessage);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectChatMessage {
    pub partner: Address,
    pub me_speaking: bool,
    pub message: String,
}
