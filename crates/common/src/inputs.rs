use bevy::{platform::collections::HashMap, prelude::*};
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

#[derive(
    Serialize, Deserialize, PartialEq, Eq, Clone, Copy, Hash, Debug, PartialOrd, Ord, EnumIter,
)]
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
    QuickEmote1,
    QuickEmote2,
    QuickEmote3,
    QuickEmote4,
    QuickEmote5,
    QuickEmote6,
    QuickEmote7,
    QuickEmote8,
    QuickEmote9,
    QuickEmote0,
}

impl From<SystemAction> for Action {
    fn from(value: SystemAction) -> Self {
        Self::System(value)
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(i32)]
pub enum CommonInputAction {
    IaPointer = 0,
    IaPrimary = 1,
    IaSecondary = 2,
    IaAny = 3,
    IaForward = 4,
    IaBackward = 5,
    IaRight = 6,
    IaLeft = 7,
    IaJump = 8,
    IaWalk = 9,
    IaAction3 = 10,
    IaAction4 = 11,
    IaAction5 = 12,
    IaAction6 = 13,
}

impl From<CommonInputAction> for Action {
    fn from(value: CommonInputAction) -> Self {
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
    Gamepad(GamepadButton),
    Analog(AxisIdentifier, InputDirection),
}

// [RIGHT, LEFT, UP, DOWN]
#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub struct InputDirectionalSet(pub [Action; 4]);

pub const MOVE_SET: InputDirectionalSet = InputDirectionalSet([
    Action::Scene(CommonInputAction::IaRight),
    Action::Scene(CommonInputAction::IaLeft),
    Action::Scene(CommonInputAction::IaForward),
    Action::Scene(CommonInputAction::IaBackward),
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
            let Ok(button) = serde_json::from_str::<GamepadButton>(&format!("\"{button}\"")) else {
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
    Scene(CommonInputAction),
    System(SystemAction),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BindingsData {
    pub bindings: HashMap<Action, Vec<InputIdentifier>>,
}

#[derive(Resource, Clone)]
pub struct InputMap {
    pub inputs: HashMap<Action, Vec<InputIdentifier>>,
}

impl Default for InputMap {
    fn default() -> Self {
        Self {
            inputs: HashMap::from_iter([
                (
                    Action::Scene(CommonInputAction::IaPointer),
                    vec![
                        InputIdentifier::Mouse(MouseButton::Left),
                        InputIdentifier::Gamepad(GamepadButton::LeftTrigger2),
                    ],
                ),
                (
                    Action::Scene(CommonInputAction::IaPrimary),
                    vec![
                        InputIdentifier::Key(KeyCode::KeyE),
                        InputIdentifier::Gamepad(GamepadButton::South),
                    ],
                ),
                (
                    Action::Scene(CommonInputAction::IaSecondary),
                    vec![
                        InputIdentifier::Key(KeyCode::KeyF),
                        InputIdentifier::Gamepad(GamepadButton::East),
                    ],
                ),
                (
                    Action::Scene(CommonInputAction::IaForward),
                    vec![
                        InputIdentifier::Key(KeyCode::KeyW),
                        InputIdentifier::Analog(AxisIdentifier::GamepadLeft, InputDirection::Up),
                    ],
                ),
                (
                    Action::Scene(CommonInputAction::IaBackward),
                    vec![
                        InputIdentifier::Key(KeyCode::KeyS),
                        InputIdentifier::Analog(AxisIdentifier::GamepadLeft, InputDirection::Down),
                    ],
                ),
                (
                    Action::Scene(CommonInputAction::IaRight),
                    vec![
                        InputIdentifier::Key(KeyCode::KeyD),
                        InputIdentifier::Analog(AxisIdentifier::GamepadLeft, InputDirection::Right),
                    ],
                ),
                (
                    Action::Scene(CommonInputAction::IaLeft),
                    vec![
                        InputIdentifier::Key(KeyCode::KeyA),
                        InputIdentifier::Analog(AxisIdentifier::GamepadLeft, InputDirection::Left),
                    ],
                ),
                (
                    Action::Scene(CommonInputAction::IaJump),
                    vec![
                        InputIdentifier::Key(KeyCode::Space),
                        InputIdentifier::Gamepad(GamepadButton::North),
                    ],
                ),
                (
                    Action::Scene(CommonInputAction::IaWalk),
                    vec![InputIdentifier::Key(KeyCode::ShiftLeft)],
                ),
                (
                    Action::Scene(CommonInputAction::IaAction3),
                    vec![
                        InputIdentifier::Key(KeyCode::Digit1),
                        InputIdentifier::Gamepad(GamepadButton::DPadUp),
                    ],
                ),
                (
                    Action::Scene(CommonInputAction::IaAction4),
                    vec![
                        InputIdentifier::Key(KeyCode::Digit2),
                        InputIdentifier::Gamepad(GamepadButton::DPadRight),
                    ],
                ),
                (
                    Action::Scene(CommonInputAction::IaAction5),
                    vec![
                        InputIdentifier::Key(KeyCode::Digit3),
                        InputIdentifier::Gamepad(GamepadButton::DPadDown),
                    ],
                ),
                (
                    Action::Scene(CommonInputAction::IaAction6),
                    vec![
                        InputIdentifier::Key(KeyCode::Digit4),
                        InputIdentifier::Gamepad(GamepadButton::DPadLeft),
                    ],
                ),
                (
                    Action::System(SystemAction::CameraLock),
                    vec![
                        InputIdentifier::Mouse(MouseButton::Right),
                        InputIdentifier::Gamepad(GamepadButton::RightTrigger2),
                    ],
                ),
                (
                    Action::System(SystemAction::Emote),
                    vec![
                        InputIdentifier::Key(KeyCode::AltLeft),
                        InputIdentifier::Gamepad(GamepadButton::West),
                    ],
                ),
                (
                    Action::System(SystemAction::Cancel),
                    vec![
                        InputIdentifier::Key(KeyCode::Escape),
                        InputIdentifier::Gamepad(GamepadButton::Select),
                    ],
                ),
                (
                    Action::System(SystemAction::HideUi),
                    vec![InputIdentifier::Key(KeyCode::PageUp)],
                ),
                (
                    Action::System(SystemAction::RollLeft),
                    vec![InputIdentifier::Key(KeyCode::KeyT)],
                ),
                (
                    Action::System(SystemAction::RollRight),
                    vec![InputIdentifier::Key(KeyCode::KeyG)],
                ),
                (
                    Action::System(SystemAction::Microphone),
                    vec![InputIdentifier::Key(KeyCode::ControlLeft)],
                ),
                (
                    Action::System(SystemAction::Chat),
                    vec![
                        InputIdentifier::Key(KeyCode::Enter),
                        InputIdentifier::Key(KeyCode::NumpadEnter),
                    ],
                ),
                (
                    Action::System(SystemAction::CameraZoomIn),
                    vec![
                        InputIdentifier::Analog(AxisIdentifier::MouseWheel, InputDirection::Up),
                        InputIdentifier::Gamepad(GamepadButton::LeftTrigger),
                    ],
                ),
                (
                    Action::System(SystemAction::CameraZoomOut),
                    vec![
                        InputIdentifier::Analog(AxisIdentifier::MouseWheel, InputDirection::Down),
                        InputIdentifier::Gamepad(GamepadButton::RightTrigger),
                    ],
                ),
                (
                    Action::System(SystemAction::ScrollUp),
                    vec![
                        InputIdentifier::Analog(AxisIdentifier::MouseWheel, InputDirection::Up),
                        InputIdentifier::Analog(AxisIdentifier::GamepadLeft, InputDirection::Up),
                    ],
                ),
                (
                    Action::System(SystemAction::ScrollDown),
                    vec![
                        InputIdentifier::Analog(AxisIdentifier::MouseWheel, InputDirection::Down),
                        InputIdentifier::Analog(AxisIdentifier::GamepadLeft, InputDirection::Down),
                    ],
                ),
                (
                    Action::System(SystemAction::ScrollLeft),
                    vec![
                        InputIdentifier::Analog(AxisIdentifier::MouseWheel, InputDirection::Left),
                        InputIdentifier::Analog(AxisIdentifier::GamepadLeft, InputDirection::Left),
                    ],
                ),
                (
                    Action::System(SystemAction::ScrollRight),
                    vec![
                        InputIdentifier::Analog(AxisIdentifier::MouseWheel, InputDirection::Right),
                        InputIdentifier::Analog(AxisIdentifier::GamepadLeft, InputDirection::Right),
                    ],
                ),
                (
                    Action::System(SystemAction::ShowProfile),
                    vec![
                        InputIdentifier::Mouse(MouseButton::Middle),
                        InputIdentifier::Gamepad(GamepadButton::North),
                    ],
                ),
                (
                    Action::System(SystemAction::PointerUp),
                    vec![InputIdentifier::Analog(
                        AxisIdentifier::GamepadRight,
                        InputDirection::Up,
                    )],
                ),
                (
                    Action::System(SystemAction::PointerDown),
                    vec![InputIdentifier::Analog(
                        AxisIdentifier::GamepadRight,
                        InputDirection::Down,
                    )],
                ),
                (
                    Action::System(SystemAction::PointerLeft),
                    vec![InputIdentifier::Analog(
                        AxisIdentifier::GamepadRight,
                        InputDirection::Left,
                    )],
                ),
                (
                    Action::System(SystemAction::PointerRight),
                    vec![InputIdentifier::Analog(
                        AxisIdentifier::GamepadRight,
                        InputDirection::Right,
                    )],
                ),
                (
                    Action::System(SystemAction::QuickEmote0),
                    vec![InputIdentifier::Key(KeyCode::Digit0)],
                ),
                (
                    Action::System(SystemAction::QuickEmote1),
                    vec![InputIdentifier::Key(KeyCode::Digit1)],
                ),
                (
                    Action::System(SystemAction::QuickEmote2),
                    vec![InputIdentifier::Key(KeyCode::Digit2)],
                ),
                (
                    Action::System(SystemAction::QuickEmote3),
                    vec![InputIdentifier::Key(KeyCode::Digit3)],
                ),
                (
                    Action::System(SystemAction::QuickEmote4),
                    vec![InputIdentifier::Key(KeyCode::Digit4)],
                ),
                (
                    Action::System(SystemAction::QuickEmote5),
                    vec![InputIdentifier::Key(KeyCode::Digit5)],
                ),
                (
                    Action::System(SystemAction::QuickEmote6),
                    vec![InputIdentifier::Key(KeyCode::Digit6)],
                ),
                (
                    Action::System(SystemAction::QuickEmote7),
                    vec![InputIdentifier::Key(KeyCode::Digit7)],
                ),
                (
                    Action::System(SystemAction::QuickEmote8),
                    vec![InputIdentifier::Key(KeyCode::Digit8)],
                ),
                (
                    Action::System(SystemAction::QuickEmote9),
                    vec![InputIdentifier::Key(KeyCode::Digit9)],
                ),
            ]),
        }
    }
}

impl InputMap {
    pub fn get_input(&self, action: CommonInputAction) -> Option<InputIdentifier> {
        self.inputs.get(&Action::Scene(action))?.first().copied()
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct InputMapSerialized(pub Vec<(Action, Vec<InputIdentifier>)>);

impl Default for InputMapSerialized {
    fn default() -> Self {
        Self(InputMap::default().inputs.into_iter().collect())
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SystemActionEvent {
    pub action: SystemAction,
    pub pressed: bool,
}
