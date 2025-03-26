pub mod settings;

use bevy::{
    app::{Plugin, Update},
    ecs::{event::EventReader, system::Local},
    prelude::{Event, EventWriter, ResMut, Resource},
};
use bevy_console::{clap::builder::StyledStr, ConsoleCommandEntered, PrintConsoleLine};
use common::{
    inputs::{BindingsData, InputIdentifier},
    rpc::RpcResultSender,
};
use dcl_component::proto_components::sdk::components::{PbAvatarBase, PbAvatarEquippedData};
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
        app.add_systems(Update, post_events);

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
) {
    while let Ok(ev) = bridge.receiver.try_recv() {
        if let SystemApi::ConsoleCommand(cmd, args, sender) = ev {
            console.send(ConsoleCommandEntered {
                command_name: cmd,
                args,
            });
            *console_response = Some(sender);
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
    } else {
        replies.clear();
    }
}
