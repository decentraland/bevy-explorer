use bevy::{
    app::{Plugin, Update},
    prelude::{Event, EventWriter, ResMut, Resource},
};
use common::rpc::RpcResultSender;

pub struct SystemBridgePlugin;

impl Plugin for SystemBridgePlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_event::<SystemApi>();
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        app.insert_resource(SystemBridge { sender, receiver });
        app.add_systems(Update, post_events);
    }
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
