pub mod settings;

use bevy::{
    app::{Plugin, Update},
    prelude::{Event, EventWriter, KeyCode, MouseButton, ResMut, Resource},
    utils::HashMap,
};
use common::rpc::RpcResultSender;
use dcl_component::proto_components::sdk::components::{
    common::InputAction, PbAvatarBase, PbAvatarEquippedData,
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

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Copy, Hash, Debug, PartialOrd, Ord)]
pub enum SystemAction {
    Cancel,
    CameraLock,
    Emote,
    HideUi,
    RollLeft,
    RollRight,
    Microphone,
    Chat,
    CameraZoomIn,
    CameraZoomOut,
    ScrollUp,
    ScrollDown,
    ScrollLeft,
    ScrollRight,
    ShowProfile,
}

impl From<SystemAction> for Action {
    fn from(value: SystemAction) -> Self {
        Self::System(value)
    }
}

impl From<InputAction> for Action {
    fn from(value: InputAction) -> Self {
        Self::Scene(value)
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub enum InputDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub enum InputIdentifier {
    Key(KeyCode),
    Mouse(MouseButton),
    MouseWheel(InputDirection),
}

impl serde::Serialize for InputIdentifier {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match *self {
            InputIdentifier::Key(ref key) => {
                let string = serde_json::to_string(key).unwrap();
                string
                    .strip_prefix("\"")
                    .unwrap()
                    .strip_suffix("\"")
                    .unwrap()
                    .serialize(serializer)
            }
            InputIdentifier::Mouse(ref button) => {
                let string = serde_json::to_string(button).unwrap();
                format!(
                    "Mouse {}",
                    string
                        .strip_prefix("\"")
                        .unwrap()
                        .strip_suffix("\"")
                        .unwrap()
                )
                .serialize(serializer)
            }
            InputIdentifier::MouseWheel(dir) => {
                let dir = serde_json::to_string(&dir).unwrap();
                let dir = dir.strip_prefix("\"").unwrap().strip_suffix("\"").unwrap();
                format!("MouseWheel {}", dir).serialize(serializer)
            }
        }
    }
}

impl<'de> serde::Deserialize<'de> for InputIdentifier {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let string = String::deserialize(deserializer)?;

        if let Some(button) = string.strip_prefix("Mouse ") {
            let Ok(button) = serde_json::from_str::<MouseButton>(&format!("\"{button}\"")) else {
                return Err(serde::de::Error::custom("invalid string"));
            };
            Ok(Self::Mouse(button))
        } else if let Some(dir) = string.strip_prefix("MouseWheel ") {
            let Ok(dir) = serde_json::from_str(&format!("\"{dir}\"")) else {
                return Err(serde::de::Error::custom("invalid string"));
            };
            Ok(Self::MouseWheel(dir))
        } else {
            let Ok(key) = serde_json::from_str::<KeyCode>(&format!("\"{string}\"")) else {
                return Err(serde::de::Error::custom("invalid string"));
            };
            Ok(Self::Key(key))
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Copy, Hash, Debug, PartialOrd, Ord)]
pub enum Action {
    Scene(InputAction),
    System(SystemAction),
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BindingsData {
    pub bindings: HashMap<Action, Vec<InputIdentifier>>,
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

pub fn post_events(mut bridge: ResMut<SystemBridge>, mut writer: EventWriter<SystemApi>) {
    while let Ok(ev) = bridge.receiver.try_recv() {
        writer.send(ev);
    }
}
