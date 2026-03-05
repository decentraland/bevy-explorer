pub mod settings;

use std::collections::VecDeque;

use bevy::{
    app::{AppExit, Plugin, Update},
    ecs::{event::EventReader, system::Local},
    log::debug,
    math::Vec4,
    prelude::{Event, EventWriter, ResMut, Resource},
};
use bevy_console::{ConsoleCommandEntered, PrintConsoleLine};
use common::{
    inputs::{BindingsData, InputIdentifier, SystemActionEvent},
    rpc::{RpcResultSender, RpcStreamSender},
    structs::{
        AppConfig, MicState, PermissionLevel, PermissionType, PermissionUsed, PermissionValue,
    },
};
use dcl_component::proto_components::{
    common::Vector2,
    sdk::components::{pb_pointer_events, PbAvatarBase, PbAvatarEquippedData},
};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use settings::SettingBridgePlugin;

use crate::settings::SettingInfo;

pub struct SystemBridgePlugin {
    pub bare: bool,
}

impl Plugin for SystemBridgePlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_event::<SystemApi>();
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        app.insert_resource(SystemBridge { sender, receiver });
        app.add_systems(Update, (post_events, handle_home_scene, handle_exit));

        if self.bare {
            return;
        }

        app.add_plugins(SettingBridgePlugin);
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SetAvatarData {
    pub base: Option<PbAvatarBase>,
    pub equip: Option<PbAvatarEquippedData>,
    pub has_claimed_name: Option<bool>,
    pub profile_extras: Option<std::collections::HashMap<String, serde_json::Value>>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LiveSceneInfo {
    pub hash: String,
    pub base_url: Option<String>,
    pub title: String,
    pub parcels: Vec<Vector2>,
    pub is_portable: bool,
    pub is_broken: bool,
    pub is_blocked: bool,
    pub is_super: bool,
    pub sdk_version: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HomeScene {
    pub realm: String,
    pub parcel: Vector2,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub sender_address: String,
    pub message: String,
    pub channel: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct VoiceMessage {
    pub sender_address: String,
    pub channel: String,
    pub active: bool,
}

#[derive(Hash, Clone, Copy, Serialize_repr, Deserialize_repr, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum PointerTargetType {
    World = 0,
    Ui = 1,
    Avatar = 2,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HoverAction {
    #[serde(flatten)]
    pub event: pb_pointer_events::Entry,
    pub enabled: bool,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HoverEvent {
    pub entered: bool,
    pub target_type: PointerTargetType,
    pub actions: Vec<HoverAction>,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SceneLoadingUi {
    pub visible: bool,
    pub title: String,
    pub pending_assets: Option<u32>,
}

#[derive(Event, Clone, Debug, Serialize, Deserialize)]
pub enum SystemApi {
    ConsoleCommand(String, Vec<String>, RpcResultSender<Result<String, String>>),
    CheckForUpdate(RpcResultSender<Option<(String, String)>>),
    MOTD(RpcResultSender<String>),
    GetPreviousLogin(RpcResultSender<Option<String>>),
    LoginPrevious(RpcResultSender<Result<(), String>>),
    LoginNew(
        RpcResultSender<Result<Option<i32>, String>>,
        RpcResultSender<Result<(), String>>,
    ),
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
    GetSceneLoadingUiStream(RpcStreamSender<SceneLoadingUi>),
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
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PermanentPermissionItem {
    pub ty: PermissionType,
    pub allow: PermissionValue,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PermissionRequest {
    pub ty: PermissionType,
    pub additional: Option<String>,
    pub scene: String,
    pub id: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SetSinglePermission {
    pub id: usize,
    pub allow: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SetPermanentPermission {
    pub ty: PermissionType,
    pub level: PermissionLevel,
    pub allow: Option<PermissionValue>,
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
    mut console_response: Local<Option<RpcResultSender<Result<String, String>>>>,
    mut replies: EventReader<PrintConsoleLine>,
    mut pending: Local<VecDeque<(String, Vec<String>, RpcResultSender<Result<String, String>>)>>,
) {
    while let Ok(ev) = bridge.receiver.try_recv() {
        if let SystemApi::ConsoleCommand(cmd, args, sender) = ev {
            debug!("system bridge (cc): {cmd} {args:?}");
            pending.push_back((cmd, args, sender));
        } else {
            debug!("system bridge: {ev:?}");
            writer.write(ev);
        }
    }

    if let Some(response) = console_response.take() {
        let mut reply = replies.read().collect::<Vec<_>>();
        match reply.pop() {
            Some(PrintConsoleLine { line }) if line.as_str() == "[ok]" => {
                response.send(Ok(reply
                    .into_iter()
                    .map(|l| l.line.clone())
                    .collect::<Vec<_>>()
                    .join("\n")));
            }
            Some(PrintConsoleLine { line }) if line.as_str() == "[failed]" => {
                response.send(Err(reply
                    .into_iter()
                    .map(|l| l.line.clone())
                    .collect::<Vec<_>>()
                    .join("\n")));
            }
            Some(PrintConsoleLine { line }) => {
                debug!("got {line}");
                *console_response = Some(response);
            }
            _ => {
                *console_response = Some(response);
            }
        }
    } else if let Some((cmd, args, sender)) = pending.pop_front() {
        console.write(ConsoleCommandEntered {
            command_name: cmd,
            args,
        });
        *console_response = Some(sender);
    }

    replies.clear();
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
