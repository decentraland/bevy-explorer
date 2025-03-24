pub mod settings;

use bevy::{
    app::{Plugin, Update},
    prelude::{Event, EventWriter, GamepadButtonType, KeyCode, MouseButton, ResMut, Resource},
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
    PointerUp,
    PointerDown,
    PointerLeft,
    PointerRight,
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

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub enum AxisIdentifier {
    MouseMove,
    MouseWheel,
    GamepadLeft,
    GamepadRight,
    GamepadLeftTrigger,
    GamepadRightTrigger,
}

#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub enum InputIdentifier {
    Key(KeyCode),
    Mouse(MouseButton),
    Gamepad(GamepadButtonType),
    Analog(AxisIdentifier, InputDirection),
}

// [RIGHT, LEFT, UP, DOWN]
#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub struct InputDirectionalSet(pub [Action; 4]);

pub const MOVE_SET: InputDirectionalSet = InputDirectionalSet([
    Action::Scene(InputAction::IaRight),
    Action::Scene(InputAction::IaLeft),
    Action::Scene(InputAction::IaForward),
    Action::Scene(InputAction::IaBackward),
]);
pub const SCROLL_SET: InputDirectionalSet = InputDirectionalSet([
    Action::System(SystemAction::ScrollRight),
    Action::System(SystemAction::ScrollLeft),
    Action::System(SystemAction::ScrollUp),
    Action::System(SystemAction::ScrollDown),
]);
pub const POINTER_SET: InputDirectionalSet = InputDirectionalSet([
    Action::System(SystemAction::PointerRight),
    Action::System(SystemAction::PointerLeft),
    Action::System(SystemAction::PointerDown),
    Action::System(SystemAction::PointerUp),
]);

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
            InputIdentifier::Gamepad(ref button) => {
                let string = serde_json::to_string(button).unwrap();
                format!(
                    "Gamepad {}",
                    string
                        .strip_prefix("\"")
                        .unwrap()
                        .strip_suffix("\"")
                        .unwrap()
                )
                .serialize(serializer)
            }
            InputIdentifier::Analog(ident, dir) => {
                let ident = serde_json::to_string(&ident).unwrap();
                let ident = ident
                    .strip_prefix("\"")
                    .unwrap()
                    .strip_suffix("\"")
                    .unwrap();
                let dir = serde_json::to_string(&dir).unwrap();
                let dir = dir.strip_prefix("\"").unwrap().strip_suffix("\"").unwrap();
                format!("{ident} {dir}").serialize(serializer)
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
        } else if let Some(button) = string.strip_prefix("Gamepad ") {
            let Ok(button) = serde_json::from_str::<GamepadButtonType>(&format!("\"{button}\""))
            else {
                return Err(serde::de::Error::custom("invalid string"));
            };
            Ok(Self::Gamepad(button))
        } else if let Some((ident, dir)) = string.split_once(" ") {
            let Ok(ident) = serde_json::from_str::<AxisIdentifier>(&format!("\"{ident}\"")) else {
                return Err(serde::de::Error::custom("invalid string"));
            };
            let Ok(dir) = serde_json::from_str(&format!("\"{dir}\"")) else {
                return Err(serde::de::Error::custom("invalid string"));
            };
            Ok(Self::Analog(ident, dir))
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
