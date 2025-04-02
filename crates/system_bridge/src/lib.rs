pub mod settings;

use std::collections::VecDeque;

use bevy::{
    app::{Plugin, Update},
    ecs::{event::EventReader, system::Local},
    prelude::{Event, EventWriter, ResMut, Resource},
};
use bevy_console::{clap::builder::StyledStr, ConsoleCommandEntered, PrintConsoleLine};
use common::{
    inputs::{BindingsData, InputIdentifier, SystemActionEvent},
    rpc::RpcResultSender,
    structs::AppConfig,
    util::config_file,
};
use dcl_component::proto_components::{
    common::Vector2,
    sdk::components::{PbAvatarBase, PbAvatarEquippedData},
};
use serde::{Deserialize, Serialize};
use settings::{SettingBridgePlugin, Settings};

pub struct SystemBridgePlugin {
    pub bare: bool,
}

impl Plugin for SystemBridgePlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_event::<SystemApi>();
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        app.insert_resource(SystemBridge { sender, receiver });
        app.add_systems(Update, (post_events, handle_home_scene));

        if self.bare {
            return;
        }

        app.add_plugins(SettingBridgePlugin);
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SetAvatarData {
    pub base: Option<PbAvatarBase>,
    pub equip: Option<PbAvatarEquippedData>,
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

#[derive(Serialize, Deserialize, Clone)]
pub struct HomeScene {
    pub realm: String,
    pub parcel: Vector2,
}

#[derive(Event, Clone)]
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
    GetSettings(RpcResultSender<Settings>),
    SetAvatar(SetAvatarData, RpcResultSender<Result<u32, String>>),
    GetNativeInput(RpcResultSender<InputIdentifier>),
    GetBindings(RpcResultSender<BindingsData>),
    SetBindings(BindingsData, RpcResultSender<()>),
    LiveSceneInfo(RpcResultSender<Vec<LiveSceneInfo>>),
    GetHomeScene(RpcResultSender<HomeScene>),
    SetHomeScene(HomeScene),
    GetSystemActionStream(tokio::sync::mpsc::UnboundedSender<SystemActionEvent>),
}

#[derive(Resource)]
pub struct NativeUi {
    pub login: bool,
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
            pending.push_back((cmd, args, sender));
        } else {
            writer.send(ev);
        }
    }

    if let Some(response) = console_response.take() {
        let mut reply = replies.read().collect::<Vec<_>>();
        match reply.pop() {
            Some(PrintConsoleLine { line }) if line == &StyledStr::from("[ok]") => {
                response.send(Ok(reply
                    .into_iter()
                    .map(|l| format!("{}", l.line))
                    .collect::<Vec<_>>()
                    .join("\n")));
            }
            Some(PrintConsoleLine { line }) if line == &StyledStr::from("[failed]") => {
                response.send(Err(reply
                    .into_iter()
                    .map(|l| format!("{}", l.line))
                    .collect::<Vec<_>>()
                    .join("\n")));
            }
            Some(PrintConsoleLine { line }) => {
                println!("got {line}");
                *console_response = Some(response);
            }
            _ => {
                *console_response = Some(response);
            }
        }
    } else if let Some((cmd, args, sender)) = pending.pop_front() {
        console.send(ConsoleCommandEntered {
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
                let config_file = config_file();
                if let Some(folder) = config_file.parent() {
                    std::fs::create_dir_all(folder).unwrap();
                }
                let _ = std::fs::write(config_file, serde_json::to_string(&*config).unwrap());
            }
            _ => (),
        }
    }
}
