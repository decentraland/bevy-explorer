pub mod agent_commands;
pub mod settings;

use std::collections::HashMap;
use std::sync::Arc;

use bevy::{
    app::{AppExit, Plugin, Update},
    ecs::event::EventReader,
    log::debug,
    math::Vec4,
    prelude::{Event, EventWriter, Res, ResMut, Resource},
};
use bevy_console::{ConsoleCommandEntered, ConsoleConfiguration, ConsoleResponder};
use common::{
    inputs::{BindingsData, InputIdentifier, SystemActionEvent},
    rpc::{RpcResultSender, RpcStreamSender},
    structs::{AppConfig, MicState, PermissionUsed},
};
use serde::{Deserialize, Serialize};
use settings::SettingBridgePlugin;
pub use system_api_types::*;

pub struct SystemBridgePlugin {
    pub bare: bool,
}

impl Plugin for SystemBridgePlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_event::<SystemApi>();
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        app.insert_resource(SystemBridge { sender, receiver });
        app.init_resource::<SceneParams>();
        app.add_systems(
            Update,
            (
                post_events,
                handle_home_scene,
                handle_exit,
                handle_get_params,
            ),
        );

        if self.bare {
            return;
        }

        app.add_plugins((SettingBridgePlugin, agent_commands::AgentCommandsPlugin));
    }
}

/// Prefix on login error strings when the failure was fetching the user's profile —
/// UIs match on this to offer retrying with a default profile (see the login variants'
/// `default_on_error` bool).
pub const PROFILE_FETCH_FAILED: &str = "profile fetch failed";

#[derive(Event, Clone, Debug, Serialize, Deserialize)]
pub enum SystemApi {
    ConsoleCommand(String, Vec<String>, RpcResultSender<Result<String, String>>),
    CheckForUpdate(RpcResultSender<Option<(String, String)>>),
    MOTD(RpcResultSender<String>),
    GetPreviousLogin(RpcResultSender<Option<String>>),
    /// bool: continue with (and deploy) a default profile if the user's current profile
    /// can't be fetched — overwrites the server-side profile, so UIs should only set it
    /// after a failed attempt and with explicit user consent.
    LoginPrevious(bool, RpcResultSender<Result<(), String>>),
    /// bool: as for LoginPrevious.
    LoginNew(
        bool,
        RpcResultSender<Result<Option<i32>, String>>,
        RpcResultSender<Result<(), String>>,
    ),
    /// Log in with an AuthIdentity the web page already holds (base64-encoded AuthIdentity
    /// JSON read from localStorage) — no auth-server round-trip. The identity is the same
    /// regardless of how the user signed in (wallet, social, OTP, magic).
    /// bool: as for LoginPrevious.
    LoginWithIdentity(String, bool, RpcResultSender<Result<(), String>>),
    LoginGuest,
    LoginCancel,
    Logout,
    GetSettings(RpcResultSender<Vec<SettingInfo>>),
    SetSetting(String, f32),
    SetAvatar(SetAvatarData, RpcResultSender<Result<u32, String>>),
    GetNativeInput(RpcResultSender<InputIdentifier>),
    GetBindings(RpcResultSender<BindingsData>),
    SetBindings(BindingsData, RpcResultSender<()>),
    LiveSceneInfo(RpcResultSender<Vec<LiveSceneInfo>>),
    GetHomeScene(RpcResultSender<HomeScene>),
    SetHomeScene(HomeScene),
    GetSystemActionStream(RpcStreamSender<SystemActionEvent>),
    GetChatStream(RpcStreamSender<ChatMessage>),
    GetVoiceStream(RpcStreamSender<VoiceMessage>),
    GetHoverStream(RpcStreamSender<HoverEvent>),
    GetProximityStream(RpcStreamSender<ProximityEvent>),
    GetSceneLoadingUiStream(RpcStreamSender<SceneLoadingUi>),
    // Native-only transport for the super-user bridge scene's BroadcastChannel: the scene posts page
    // -bound Envelopes via BridgeToPage, and subscribes to page->scene Envelopes via GetBridgeStream.
    // (In web the browser provides BroadcastChannel directly; native has no cross-process equivalent.)
    BridgeToPage(String),
    GetBridgeStream(RpcStreamSender<String>),
    SendChat(String, String),
    Quit,
    GetPermissionRequestStream(RpcStreamSender<PermissionRequest>),
    SetSinglePermission(SetSinglePermission),
    SetPermanentPermission(SetPermanentPermission),
    GetPermissionUsedStream(RpcStreamSender<PermissionUsed>),
    GetPermanentPermissions(
        PermissionLevel,
        RpcResultSender<Vec<PermanentPermissionItem>>,
    ),
    SetInteractableArea(Vec4),
    GetMicState(RpcResultSender<MicState>),
    SetMicEnabled(bool),
    GetAvatarModifiers(RpcResultSender<Vec<AvatarModifierState>>),
    // Social / Friends
    GetFriends(RpcResultSender<Vec<FriendData>>),
    GetSentFriendRequests(RpcResultSender<Vec<FriendRequestData>>),
    GetReceivedFriendRequests(RpcResultSender<Vec<FriendRequestData>>),
    GetSocialInitialized(RpcResultSender<bool>),
    SendFriendRequest(String, Option<String>, RpcResultSender<Result<(), String>>),
    AcceptFriendRequest(String, RpcResultSender<Result<(), String>>),
    RejectFriendRequest(String, RpcResultSender<Result<(), String>>),
    CancelFriendRequest(String, RpcResultSender<Result<(), String>>),
    DeleteFriend(String, RpcResultSender<Result<(), String>>),
    GetFriendshipEventStream(RpcStreamSender<FriendshipEventUpdate>),
    GetMutualFriends(String, RpcResultSender<Vec<FriendData>>),
    GetOnlineFriends(RpcResultSender<Vec<FriendStatusData>>),
    GetFriendConnectivityStream(RpcStreamSender<FriendConnectivityEvent>),
    // Social / Blocking
    BlockUser(String, RpcResultSender<Result<(), String>>),
    UnblockUser(String, RpcResultSender<Result<(), String>>),
    GetBlockedUsers(RpcResultSender<Vec<BlockedUserData>>),
    GetBlockingStatus(RpcResultSender<Result<BlockingStatusData, String>>),
    GetBlockUpdateStream(RpcStreamSender<BlockUpdateData>),
    GetParams(RpcResultSender<HashMap<String, String>>),
}

#[derive(Resource, Default, Clone, Debug)]
pub struct SceneParams(pub HashMap<String, String>);

impl SceneParams {
    pub fn from_query_string(query: &str, decode: bool) -> Self {
        let map = query
            .split('&')
            .filter(|s| !s.is_empty())
            .filter_map(|pair| {
                let mut parts = pair.splitn(2, '=');
                let key = parts.next()?.to_owned();
                let value = parts.next().unwrap_or("").to_owned();
                if decode {
                    Some((
                        urlencoding::decode(&key).unwrap_or_default().into_owned(),
                        urlencoding::decode(&value).unwrap_or_default().into_owned(),
                    ))
                } else {
                    Some((key, value))
                }
            })
            .collect();
        Self(map)
    }
}

#[derive(Resource)]
pub struct NativeUi {
    pub login: bool,
    pub emote_wheel: bool,
    pub chat: bool,
    pub permissions: bool,
    pub profile: bool,
    pub nametags: bool,
    pub tooltips: bool,
    pub loading_scene: bool,
}

#[derive(Resource)]
pub struct SystemBridge {
    pub sender: tokio::sync::mpsc::UnboundedSender<SystemApi>,
    receiver: tokio::sync::mpsc::UnboundedReceiver<SystemApi>,
}

pub fn post_events(
    mut bridge: ResMut<SystemBridge>,
    mut writer: EventWriter<SystemApi>,
    mut console: EventWriter<ConsoleCommandEntered>,
    console_config: Res<ConsoleConfiguration>,
) {
    while let Ok(ev) = bridge.receiver.try_recv() {
        let SystemApi::ConsoleCommand(cmd, args, sender) = ev else {
            debug!("system bridge: {ev:?}");
            writer.write(ev);
            continue;
        };

        debug!("system bridge (cc): {cmd} {args:?}");

        // Dispatch as a `ConsoleCommandEntered` carrying its own responder, so the result
        // flows back through the channel rather than being scraped from shared console
        // output. `bevy_console` queues concurrent invocations and refires any it can't
        // process this frame, so there is no need to serialize dispatch here.
        if console_config.commands.contains_key(cmd.as_str()) {
            let responder: ConsoleResponder = Arc::new(move |result| sender.send(result));
            console.write(ConsoleCommandEntered {
                command_name: cmd,
                args,
                responder: Some(responder),
            });
        } else {
            sender.send(Err(format!(
                "Command not recognized: `{cmd}`. Recognized commands: {:?}",
                console_config.commands.keys().collect::<Vec<_>>()
            )));
        }
    }
}

fn handle_home_scene(mut ev: EventReader<SystemApi>, mut config: ResMut<AppConfig>) {
    for ev in ev.read() {
        match ev {
            SystemApi::GetHomeScene(rpc_result_sender) => rpc_result_sender.send(HomeScene {
                realm: config.server.clone(),
                parcel: config.location.as_vec2().into(),
            }),
            SystemApi::SetHomeScene(home_scene) => {
                config.server = home_scene.realm.clone();
                config.location = bevy::math::Vec2::from(&home_scene.parcel).as_ivec2();
                platform::write_config_file(&*config);
            }
            _ => (),
        }
    }
}

fn handle_exit(mut ev: EventReader<SystemApi>, mut exit: EventWriter<AppExit>) {
    if ev
        .read()
        .filter(|e| matches!(e, SystemApi::Quit))
        .last()
        .is_some()
    {
        exit.write_default();
    }
}

fn handle_get_params(mut ev: EventReader<SystemApi>, params: Res<SceneParams>) {
    for ev in ev.read() {
        if let SystemApi::GetParams(sender) = ev {
            sender.send(params.0.clone());
        }
    }
}
