// input settings

use bimap::BiMap;

use bevy::{ecs::system::SystemParam, prelude::*, window::PrimaryWindow};
use bevy_console::ConsoleOpen;
use bevy_egui::EguiContext;

use dcl_component::proto_components::sdk::components::common::InputAction;

pub struct InputManagerPlugin;

impl Plugin for InputManagerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InputMap>();
        app.init_resource::<AcceptInput>();
        app.add_systems(PreUpdate, check_accept_input);
    }
}

#[derive(Resource)]
pub struct InputMap {
    inputs: BiMap<InputAction, InputItem>,
}

impl Default for InputMap {
    fn default() -> Self {
        Self {
            inputs: BiMap::from_iter(
                [
                    (InputAction::IaPointer, InputItem::Mouse(MouseButton::Left)),
                    (InputAction::IaPrimary, InputItem::Key(KeyCode::E)),
                    (InputAction::IaSecondary, InputItem::Key(KeyCode::F)),
                    (InputAction::IaForward, InputItem::Key(KeyCode::W)),
                    (InputAction::IaBackward, InputItem::Key(KeyCode::S)),
                    (InputAction::IaRight, InputItem::Key(KeyCode::D)),
                    (InputAction::IaLeft, InputItem::Key(KeyCode::A)),
                    (InputAction::IaJump, InputItem::Key(KeyCode::Space)),
                    (InputAction::IaWalk, InputItem::Key(KeyCode::ShiftLeft)),
                    (InputAction::IaAction3, InputItem::Key(KeyCode::Key1)),
                    (InputAction::IaAction4, InputItem::Key(KeyCode::Key2)),
                    (InputAction::IaAction5, InputItem::Key(KeyCode::Key3)),
                    (InputAction::IaAction6, InputItem::Key(KeyCode::Key4)),
                ]
                .into_iter(),
            ),
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
    mouse_input: Res<'w, Input<MouseButton>>,
    key_input: Res<'w, Input<KeyCode>>,
    should_accept: Res<'w, AcceptInput>,
}

impl<'w> InputManager<'w> {
    pub fn just_down(&self, action: InputAction) -> bool {
        if !self.should_accept.0 {
            return false;
        }
        self.map
            .inputs
            .get_by_left(&action)
            .map_or(false, |item| match item {
                InputItem::Key(k) => self.key_input.just_pressed(*k),
                InputItem::Mouse(mb) => self.mouse_input.just_pressed(*mb),
            })
    }

    pub fn just_up(&self, action: InputAction) -> bool {
        if !self.should_accept.0 {
            return false;
        }
        self.map
            .inputs
            .get_by_left(&action)
            .map_or(false, |item| match item {
                InputItem::Key(k) => self.key_input.just_released(*k),
                InputItem::Mouse(mb) => self.mouse_input.just_released(*mb),
            })
    }

    pub fn is_down(&self, action: InputAction) -> bool {
        if !self.should_accept.0 {
            return false;
        }
        self.map
            .inputs
            .get_by_left(&action)
            .map_or(false, |item| match item {
                InputItem::Key(k) => self.key_input.pressed(*k),
                InputItem::Mouse(mb) => self.mouse_input.pressed(*mb),
            })
    }

    pub fn iter_just_down(&self) -> impl Iterator<Item = &InputAction> {
        self.map
            .inputs
            .iter()
            .filter(|(_, button)| match button {
                InputItem::Key(k) => self.key_input.just_pressed(*k),
                InputItem::Mouse(m) => self.mouse_input.just_pressed(*m),
            })
            .filter(|_| self.should_accept.0)
            .map(|(action, _)| action)
    }

    pub fn iter_just_up(&self) -> impl Iterator<Item = &InputAction> {
        self.map
            .inputs
            .iter()
            .filter(|(_, button)| match button {
                InputItem::Key(k) => self.key_input.just_released(*k),
                InputItem::Mouse(m) => self.mouse_input.just_released(*m),
            })
            .filter(|_| self.should_accept.0)
            .map(|(action, _)| action)
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub enum InputItem {
    Key(KeyCode),
    Mouse(MouseButton),
}

impl std::fmt::Display for InputItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InputItem::Key(k) => f.write_str(key_to_str(k).as_str()),
            InputItem::Mouse(m) => f.write_fmt(format_args!("{:?}", m)),
        }
    }
}

// todo extend this when we make rebindable keys
fn key_to_str(key: &KeyCode) -> String {
    use KeyCode::*;
    let str = match key {
        Key1 => "1",
        Key2 => "2",
        Key3 => "3",
        Key4 => "4",
        Space => "Space",
        ShiftLeft => "Left Shift",
        _ => return format!("{:?}", key),
    };
    str.to_owned()
}

#[derive(Resource, Default)]
pub struct AcceptInput(pub bool);

fn check_accept_input(
    console: Res<ConsoleOpen>,
    mut ctx: Query<&mut EguiContext, With<PrimaryWindow>>,
    mut should_accept: ResMut<AcceptInput>,
) {
    let Ok(mut ctx) = ctx.get_single_mut() else {
        return;
    };
    should_accept.0 = !console.open && !ctx.get_mut().wants_keyboard_input();
}
