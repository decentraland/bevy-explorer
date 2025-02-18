pub mod settings;

use bevy::{
    app::{Plugin, Update},
    prelude::{Event, EventWriter, ResMut, Resource},
};
use common::rpc::RpcResultSender;
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

pub fn post_events(mut bridge: ResMut<SystemBridge>, mut writer: EventWriter<SystemApi>) {
    while let Ok(ev) = bridge.receiver.try_recv() {
        writer.send(ev);
    }
}
