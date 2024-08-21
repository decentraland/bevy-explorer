// input settings

use bimap::BiMap;

use bevy::{ecs::system::SystemParam, prelude::*, ui::UiSystem, window::PrimaryWindow};
use bevy_console::ConsoleOpen;
use bevy_egui::EguiContext;

use dcl_component::proto_components::sdk::components::common::InputAction;
use ui_core::ui_actions::UiActionSet;

pub struct InputManagerPlugin;

impl Plugin for InputManagerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InputMap>();
        app.init_resource::<AcceptInput>();
        app.add_systems(
            PreUpdate,
            check_accept_input
                .after(UiSystem::Focus)
                .before(UiActionSet),
        );
    }
}

// marker to attach to components that pass mouse input through to scenes
#[derive(Component)]
pub struct MouseInteractionComponent;

#[derive(Resource)]
pub struct InputMap {
    inputs: BiMap<InputAction, InputItem>,
}

impl Default for InputMap {
    fn default() -> Self {
        Self {
            inputs: BiMap::from_iter([
                (InputAction::IaAny, InputItem::Any),
                (InputAction::IaPointer, InputItem::Mouse(MouseButton::Left)),
                (InputAction::IaPrimary, InputItem::Key(KeyCode::KeyE)),
                (InputAction::IaSecondary, InputItem::Key(KeyCode::KeyF)),
                (InputAction::IaForward, InputItem::Key(KeyCode::KeyW)),
                (InputAction::IaBackward, InputItem::Key(KeyCode::KeyS)),
                (InputAction::IaRight, InputItem::Key(KeyCode::KeyD)),
                (InputAction::IaLeft, InputItem::Key(KeyCode::KeyA)),
                (InputAction::IaJump, InputItem::Key(KeyCode::Space)),
                (InputAction::IaWalk, InputItem::Key(KeyCode::ShiftLeft)),
                (InputAction::IaAction3, InputItem::Key(KeyCode::Digit1)),
                (InputAction::IaAction4, InputItem::Key(KeyCode::Digit2)),
                (InputAction::IaAction5, InputItem::Key(KeyCode::Digit3)),
                (InputAction::IaAction6, InputItem::Key(KeyCode::Digit4)),
            ]),
        }
    }
}

impl InputMap {
    pub fn get_input(&self, action: InputAction) -> InputItem {
        *self.inputs.get_by_left(&action).unwrap()
    }
}

#[derive(SystemParam)]
pub struct InputManager<'w> {
    map: Res<'w, InputMap>,
    mouse_input: Res<'w, ButtonInput<MouseButton>>,
    key_input: Res<'w, ButtonInput<KeyCode>>,
    should_accept: Res<'w, AcceptInput>,
}

impl<'w> InputManager<'w> {
    pub fn any_just_acted(&self) -> bool {
        self.mouse_input.get_just_pressed().len() != 0
            || self.mouse_input.get_just_released().len() != 0
            || self.key_input.get_just_pressed().len() != 0
            || self.key_input.get_just_released().len() != 0
    }

    pub fn just_down(&self, action: InputAction) -> bool {
        self.map
            .inputs
            .get_by_left(&action)
            .map_or(false, |item| match item {
                InputItem::Key(k) => self.should_accept.key && self.key_input.just_pressed(*k),
                InputItem::Mouse(mb) => {
                    self.should_accept.mouse && self.mouse_input.just_pressed(*mb)
                }
                InputItem::Any => self.iter_just_down().next().is_some(),
            })
    }

    pub fn just_up(&self, action: InputAction) -> bool {
        self.map
            .inputs
            .get_by_left(&action)
            .map_or(false, |item| match item {
                InputItem::Key(k) => self.key_input.just_released(*k),
                InputItem::Mouse(mb) => self.mouse_input.just_released(*mb),
                InputItem::Any => self.iter_just_up().next().is_some(),
            })
    }

    pub fn is_down(&self, action: InputAction) -> bool {
        self.map
            .inputs
            .get_by_left(&action)
            .map_or(false, |item| match item {
                InputItem::Key(k) => self.should_accept.key && self.key_input.pressed(*k),
                InputItem::Mouse(mb) => self.should_accept.mouse && self.mouse_input.pressed(*mb),
                InputItem::Any => self.iter_down().next().is_some(),
            })
    }

    pub fn iter_just_down(&self) -> impl Iterator<Item = &InputAction> {
        self.map
            .inputs
            .iter()
            .filter(|(_, button)| match button {
                InputItem::Key(k) => self.should_accept.key && self.key_input.just_pressed(*k),
                InputItem::Mouse(m) => self.mouse_input.just_pressed(*m),
                InputItem::Any => false,
            })
            .map(|(action, _)| action)
    }

    pub fn iter_just_up(&self) -> impl Iterator<Item = &InputAction> {
        self.map
            .inputs
            .iter()
            .filter(|(_, button)| match button {
                InputItem::Key(k) => self.key_input.just_released(*k),
                InputItem::Mouse(m) => self.mouse_input.just_released(*m),
                InputItem::Any => false,
            })
            .map(|(action, _)| action)
    }

    pub fn iter_down(&self) -> impl Iterator<Item = &InputAction> {
        self.map
            .inputs
            .iter()
            .filter(|(_, button)| match button {
                InputItem::Key(k) => self.should_accept.key && self.key_input.pressed(*k),
                InputItem::Mouse(m) => self.should_accept.mouse && self.mouse_input.pressed(*m),
                InputItem::Any => false,
            })
            .map(|(action, _)| action)
    }

    pub fn iter_up(&self) -> impl Iterator<Item = &InputAction> {
        self.map
            .inputs
            .iter()
            .filter(|(_, button)| match button {
                InputItem::Key(k) => self.key_input.just_released(*k),
                InputItem::Mouse(m) => self.mouse_input.just_released(*m),
                InputItem::Any => false,
            })
            .map(|(action, _)| action)
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub enum InputItem {
    Key(KeyCode),
    Mouse(MouseButton),
    Any,
}

impl std::fmt::Display for InputItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InputItem::Key(k) => f.write_str(key_to_str(k).as_str()),
            InputItem::Mouse(m) => f.write_fmt(format_args!("{:?}", m)),
            InputItem::Any => f.write_str("(Any)"),
        }
    }
}

// todo extend this when we make rebindable keys
fn key_to_str(key: &KeyCode) -> String {
    use KeyCode::*;
    let str = match key {
        Digit1 => "1",
        Digit2 => "2",
        Digit3 => "3",
        Digit4 => "4",
        Space => "Space",
        ShiftLeft => "Left Shift",
        _ => return format!("{:?}", key),
    };
    str.to_owned()
}

#[derive(Resource, Default)]
pub struct AcceptInput {
    pub mouse: bool,
    pub key: bool,
}

fn check_accept_input(
    ui_roots: Query<&Interaction, With<MouseInteractionComponent>>,
    console: Res<ConsoleOpen>,
    mut ctx: Query<&mut EguiContext, With<PrimaryWindow>>,
    mut should_accept: ResMut<AcceptInput>,
) {
    let Ok(mut ctx) = ctx.get_single_mut() else {
        return;
    };
    // we only accept mouse input if the cursor reaches the ui root, not if blocked by anything inbetween
    should_accept.mouse = ui_roots
        .iter()
        .any(|root| !matches!(root, Interaction::None));
    should_accept.key = !console.open && !ctx.get_mut().wants_keyboard_input();
}

pub fn should_accept_key(should_accept: Res<AcceptInput>) -> bool {
    should_accept.key
}

pub fn should_accept_mouse(should_accept: Res<AcceptInput>) -> bool {
    should_accept.mouse
}

pub fn should_accept_any(should_accept: Res<AcceptInput>) -> bool {
    should_accept.mouse || should_accept.key
}
