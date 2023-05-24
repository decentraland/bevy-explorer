pub mod camera;
pub mod dynamics;
pub mod player_input;

use bevy::{ecs::system::SystemParam, prelude::*, utils::HashMap};
use bevy_console::ConsoleOpen;

use crate::{
    dcl_component::proto_components::sdk::components::common::InputAction,
    scene_runner::{PrimaryUser, SceneSets},
};

use self::{
    camera::{update_camera, update_camera_position, PrimaryCamera},
    dynamics::update_user_position,
    player_input::update_user_velocity,
};

// plugin to pass user input messages to the scene
pub struct UserInputPlugin;

impl Plugin for UserInputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InputMap>();
        app.add_systems(
            (
                update_camera.run_if(|console: Res<ConsoleOpen>| !console.open),
                update_camera_position,
                update_user_velocity.run_if(|console: Res<ConsoleOpen>| !console.open),
                update_user_position,
            )
                .chain()
                .in_set(SceneSets::Input),
        );
        app.add_system(hide_player_in_first_person);
    }
}

// TODO move me somewhere sensible
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

fn hide_player_in_first_person(
    camera: Query<&PrimaryCamera>,
    mut player: Query<&mut Visibility, With<PrimaryUser>>,
) {
    if let (Ok(cam), Ok(mut vis)) = (camera.get_single(), player.get_single_mut()) {
        if cam.distance < 0.1 && *vis != Visibility::Hidden {
            *vis = Visibility::Hidden;
        } else if cam.distance > 0.1 && *vis != Visibility::Inherited {
            *vis = Visibility::Inherited;
        }
    }
}
