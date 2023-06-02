// input settings

use bevy::{ecs::system::SystemParam, prelude::*, utils::HashMap, window::PrimaryWindow};
use bevy_console::ConsoleOpen;
use bevy_egui::EguiContext;

use crate::dcl_component::proto_components::sdk::components::common::InputAction;

pub struct InputManagerPlugin;

impl Plugin for InputManagerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InputMap>();
        app.init_resource::<AcceptInput>();
        app.add_system(check_accept_input.in_base_set(CoreSet::PreUpdate));
    }
}

#[derive(Resource)]
pub struct InputMap {
    inputs: HashMap<InputAction, InputItem>,
}

impl Default for InputMap {
    fn default() -> Self {
        Self {
            inputs: HashMap::from_iter(
                [
                    (InputAction::IaPointer, InputItem::Mouse(MouseButton::Left)),
                    (InputAction::IaPrimary, InputItem::Key(KeyCode::E)),
                    (InputAction::IaSecondary, InputItem::Key(KeyCode::F)),
                    (InputAction::IaForward, InputItem::Key(KeyCode::W)),
                    (InputAction::IaBackward, InputItem::Key(KeyCode::S)),
                    (InputAction::IaRight, InputItem::Key(KeyCode::D)),
                    (InputAction::IaLeft, InputItem::Key(KeyCode::A)),
                    (InputAction::IaJump, InputItem::Key(KeyCode::Space)),
                    (InputAction::IaWalk, InputItem::Key(KeyCode::LShift)),
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

#[derive(SystemParam)]
pub struct InputManager<'w> {
    map: Res<'w, InputMap>,
    mouse_input: Res<'w, Input<MouseButton>>,
    key_input: Res<'w, Input<KeyCode>>,
}

impl<'w> InputManager<'w> {
    pub fn just_down(&self, action: InputAction) -> bool {
        self.map
            .inputs
            .get(&action)
            .map_or(false, |item| match item {
                InputItem::Key(k) => self.key_input.just_pressed(*k),
                InputItem::Mouse(mb) => self.mouse_input.just_pressed(*mb),
            })
    }

    pub fn just_up(&self, action: InputAction) -> bool {
        self.map
            .inputs
            .get(&action)
            .map_or(false, |item| match item {
                InputItem::Key(k) => self.key_input.just_released(*k),
                InputItem::Mouse(mb) => self.mouse_input.just_released(*mb),
            })
    }

    pub fn is_down(&self, action: InputAction) -> bool {
        self.map
            .inputs
            .get(&action)
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
            .map(|(action, _)| action)
    }
}

pub enum InputItem {
    Key(KeyCode),
    Mouse(MouseButton),
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
