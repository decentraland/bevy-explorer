// input settings

use std::collections::BTreeSet;

use bevy::{ecs::system::SystemParam, input::mouse::MouseWheel, prelude::*, utils::HashMap};

use common::rpc::RpcResultSender;
use dcl_component::proto_components::sdk::components::common::InputAction;
pub use system_bridge::{Action, SystemAction};
use system_bridge::{BindingsData, InputDirection, InputIdentifier, SystemApi};

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Default, Debug)]
#[repr(u32)]
pub enum InputPriority {
    #[default]
    None,
    Scene,
    Focus,
    AvatarCollider,
    TextEntry,
    Scroll,
    CancelFocus,
    BindInput,
}

#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub enum InputType {
    All,
    Keyboard,
    Action(Action),
}

#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct InputPriorities {
    reserved: HashMap<InputType, BTreeSet<InputPriority>>,
}

impl InputPriorities {
    pub fn reserve(&mut self, ty: InputType, level: InputPriority) {
        self.reserved.entry(ty).or_default().insert(level);
    }

    pub fn release(&mut self, ty: InputType, level: InputPriority) {
        if let Some(set) = self.reserved.get_mut(&ty) {
            set.remove(&level);
            if set.is_empty() {
                self.reserved.remove(&ty);
            }
        }
    }

    pub fn release_all(&mut self, level: InputPriority) {
        self.reserved.retain(|_, set| {
            set.remove(&level);
            !set.is_empty()
        })
    }

    pub fn get(&self, ty: InputType) -> InputPriority {
        self.reserved
            .get(&ty)
            .and_then(|set| set.iter().next_back())
            .copied()
            .unwrap_or(InputPriority::None)
    }
}

pub struct InputManagerPlugin;

impl Plugin for InputManagerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InputMap>();
        app.init_resource::<InputPriorities>();
        app.init_resource::<CumulativeWheelData>();
        app.init_resource::<CurrentNativeInputRequest>();
        app.add_systems(
            PreUpdate,
            (
                update_deltas,
                handle_native_input,
                handle_get_bindings,
                handle_set_bindings,
            ),
        );
    }
}

// marker to attach to components that pass mouse input through to scenes
#[derive(Component)]
pub struct MouseInteractionComponent;

#[derive(Resource)]
pub struct InputMap {
    inputs: HashMap<Action, Vec<InputIdentifier>>,
}

impl Default for InputMap {
    fn default() -> Self {
        Self {
            inputs: HashMap::from_iter([
                (
                    Action::Scene(InputAction::IaPointer),
                    vec![InputIdentifier::Mouse(MouseButton::Left)],
                ),
                (
                    Action::Scene(InputAction::IaPrimary),
                    vec![InputIdentifier::Key(KeyCode::KeyE)],
                ),
                (
                    Action::Scene(InputAction::IaSecondary),
                    vec![InputIdentifier::Key(KeyCode::KeyF)],
                ),
                (
                    Action::Scene(InputAction::IaForward),
                    vec![InputIdentifier::Key(KeyCode::KeyW)],
                ),
                (
                    Action::Scene(InputAction::IaBackward),
                    vec![InputIdentifier::Key(KeyCode::KeyS)],
                ),
                (
                    Action::Scene(InputAction::IaRight),
                    vec![InputIdentifier::Key(KeyCode::KeyD)],
                ),
                (
                    Action::Scene(InputAction::IaLeft),
                    vec![InputIdentifier::Key(KeyCode::KeyA)],
                ),
                (
                    Action::Scene(InputAction::IaJump),
                    vec![InputIdentifier::Key(KeyCode::Space)],
                ),
                (
                    Action::Scene(InputAction::IaWalk),
                    vec![InputIdentifier::Key(KeyCode::ShiftLeft)],
                ),
                (
                    Action::Scene(InputAction::IaAction3),
                    vec![InputIdentifier::Key(KeyCode::Digit1)],
                ),
                (
                    Action::Scene(InputAction::IaAction4),
                    vec![InputIdentifier::Key(KeyCode::Digit2)],
                ),
                (
                    Action::Scene(InputAction::IaAction5),
                    vec![InputIdentifier::Key(KeyCode::Digit3)],
                ),
                (
                    Action::Scene(InputAction::IaAction6),
                    vec![InputIdentifier::Key(KeyCode::Digit4)],
                ),
                (
                    Action::System(SystemAction::CameraLock),
                    vec![InputIdentifier::Mouse(MouseButton::Right)],
                ),
                (
                    Action::System(SystemAction::Emote),
                    vec![InputIdentifier::Key(KeyCode::AltLeft)],
                ),
                (
                    Action::System(SystemAction::Cancel),
                    vec![InputIdentifier::Key(KeyCode::Escape)],
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
                    vec![InputIdentifier::MouseWheel(InputDirection::Up)],
                ),
                (
                    Action::System(SystemAction::CameraZoomOut),
                    vec![InputIdentifier::MouseWheel(InputDirection::Down)],
                ),
                (
                    Action::System(SystemAction::ScrollUp),
                    vec![InputIdentifier::MouseWheel(InputDirection::Up)],
                ),
                (
                    Action::System(SystemAction::ScrollDown),
                    vec![InputIdentifier::MouseWheel(InputDirection::Down)],
                ),
                (
                    Action::System(SystemAction::ScrollLeft),
                    vec![InputIdentifier::MouseWheel(InputDirection::Left)],
                ),
                (
                    Action::System(SystemAction::ScrollRight),
                    vec![InputIdentifier::MouseWheel(InputDirection::Right)],
                ),
                (
                    Action::System(SystemAction::ShowProfile),
                    vec![InputIdentifier::Mouse(MouseButton::Middle)],
                ),
            ]),
        }
    }
}

impl InputMap {
    pub fn get_input(&self, action: InputAction) -> Option<InputIdentifier> {
        self.inputs.get(&Action::Scene(action))?.first().copied()
    }
}

#[derive(Resource, Default)]
pub struct CumulativeWheelData {
    current: Option<Vec2>,
    prev: Option<Vec2>,
}

impl CumulativeWheelData {
    fn _analog(vec: Option<Vec2>, dir: InputDirection) -> f32 {
        let vec = vec.unwrap_or_default();
        match dir {
            InputDirection::Up => vec.y.max(0.0),
            InputDirection::Down => -vec.y.min(0.0),
            InputDirection::Left => -vec.x.min(0.0),
            InputDirection::Right => vec.x.max(0.0),
        }
    }

    fn check_dir(vec: Option<Vec2>, dir: InputDirection) -> bool {
        Self::_analog(vec, dir) > 0.0
    }

    pub fn just_down(&self, dir: InputDirection) -> bool {
        Self::check_dir(self.current, dir) && !Self::check_dir(self.prev, dir)
    }

    pub fn just_up(&self, dir: InputDirection) -> bool {
        !Self::check_dir(self.current, dir) && Self::check_dir(self.prev, dir)
    }

    pub fn down(&self, dir: InputDirection) -> bool {
        Self::check_dir(self.current, dir)
    }

    pub fn analog(&self, dir: InputDirection) -> f32 {
        Self::_analog(self.current, dir)
    }
}

#[derive(SystemParam)]
pub struct InputManager<'w> {
    map: Res<'w, InputMap>,
    mouse_input: Res<'w, ButtonInput<MouseButton>>,
    key_input: Res<'w, ButtonInput<KeyCode>>,
    wheel_data: Res<'w, CumulativeWheelData>,
    priorities: ResMut<'w, InputPriorities>,
}

impl InputManager<'_> {
    pub fn priorities(&mut self) -> &mut InputPriorities {
        &mut self.priorities
    }

    pub fn any_just_acted(&self) -> bool {
        self.mouse_input.get_just_pressed().len() != 0
            || self.mouse_input.get_just_released().len() != 0
            || self.key_input.get_just_pressed().len() != 0
            || self.key_input.get_just_released().len() != 0
    }

    fn inputs(&self, action: Action) -> impl Iterator<Item = &InputIdentifier> {
        self.map
            .inputs
            .iter()
            .filter(move |(a, _)| {
                (**a == action)
                    || (matches!(a, Action::Scene(_))
                        && action == Action::Scene(InputAction::IaAny))
            })
            .flat_map(|(_, v)| v.iter())
    }

    pub fn check_priority(&self, input: &InputIdentifier, priority: InputPriority) -> bool {
        if self.priorities.get(InputType::All) > priority {
            return false;
        }

        if matches!(input, InputIdentifier::Key(_))
            && self.priorities.get(InputType::Keyboard) > priority
        {
            return false;
        }

        self.priorities.reserved.iter().all(|(k, v)| match k {
            InputType::Action(a) => self
                .inputs(*a)
                .all(|i| (i != input) || (v.iter().next_back() <= Some(&priority))),
            _ => true,
        })
    }

    pub fn just_down<T: Into<Action>>(&self, action: T, priority: InputPriority) -> bool {
        self.inputs(action.into()).any(|item| match item {
            InputIdentifier::Key(k) => {
                self.key_input.just_pressed(*k) && self.check_priority(item, priority)
            }
            InputIdentifier::Mouse(mb) => {
                self.mouse_input.just_pressed(*mb) && self.check_priority(item, priority)
            }
            InputIdentifier::MouseWheel(input_direction) => {
                self.wheel_data.just_down(*input_direction) && self.check_priority(item, priority)
            }
        })
    }

    pub fn just_up<T: Into<Action>>(&self, action: T) -> bool {
        self.inputs(action.into()).any(|item| match item {
            InputIdentifier::Key(k) => self.key_input.just_released(*k),
            InputIdentifier::Mouse(mb) => self.mouse_input.just_released(*mb),
            InputIdentifier::MouseWheel(input_direction) => {
                self.wheel_data.just_up(*input_direction)
            }
        })
    }

    pub fn is_down<T: Into<Action>>(&self, action: T, priority: InputPriority) -> bool {
        self.inputs(action.into()).any(|item| match item {
            InputIdentifier::Key(k) => {
                self.key_input.pressed(*k) && self.check_priority(item, priority)
            }
            InputIdentifier::Mouse(mb) => {
                self.mouse_input.pressed(*mb) && self.check_priority(item, priority)
            }
            InputIdentifier::MouseWheel(input_direction) => {
                self.wheel_data.down(*input_direction) && self.check_priority(item, priority)
            }
        })
    }

    pub fn down_analog<T: Into<Action>>(&self, action: T, priority: InputPriority) -> f32 {
        self.inputs(action.into())
            .map(|item| match item {
                InputIdentifier::Key(k) => {
                    if self.key_input.pressed(*k) && self.check_priority(item, priority) {
                        1.0
                    } else {
                        0.0
                    }
                }
                InputIdentifier::Mouse(mb) => {
                    if self.mouse_input.pressed(*mb) && self.check_priority(item, priority) {
                        1.0
                    } else {
                        0.0
                    }
                }
                InputIdentifier::MouseWheel(input_direction) => {
                    let analog = self.wheel_data.analog(*input_direction);
                    if analog > 0.0 && self.check_priority(item, priority) {
                        analog
                    } else {
                        0.0
                    }
                }
            })
            .sum()
    }

    // only scene actions
    pub fn iter_scene_just_down(&self) -> impl Iterator<Item = &InputAction> {
        self.map
            .inputs
            .iter()
            .filter(|(_, buttons)| {
                buttons.iter().any(|button| match button {
                    InputIdentifier::Key(k) => {
                        self.key_input.just_pressed(*k)
                            && self.check_priority(button, InputPriority::Scene)
                    }
                    InputIdentifier::Mouse(m) => {
                        self.mouse_input.just_pressed(*m)
                            && self.check_priority(button, InputPriority::Scene)
                    }
                    InputIdentifier::MouseWheel(input_direction) => {
                        self.wheel_data.just_down(*input_direction)
                            && self.check_priority(button, InputPriority::Scene)
                    }
                })
            })
            .flat_map(|(action, _)| {
                if let Action::Scene(ia) = action {
                    Some(ia)
                } else {
                    None
                }
            })
    }

    pub fn iter_scene_just_up(&self) -> impl Iterator<Item = &InputAction> {
        self.map
            .inputs
            .iter()
            .filter(|(_, buttons)| {
                buttons.iter().any(|button| match button {
                    InputIdentifier::Key(k) => self.key_input.just_released(*k),
                    InputIdentifier::Mouse(m) => self.mouse_input.just_released(*m),
                    InputIdentifier::MouseWheel(input_direction) => {
                        self.wheel_data.just_up(*input_direction)
                    }
                })
            })
            .flat_map(|(action, _)| {
                if let Action::Scene(ia) = action {
                    Some(ia)
                } else {
                    None
                }
            })
    }
}

#[derive(Resource, Default)]
struct CurrentNativeInputRequest(Option<RpcResultSender<InputIdentifier>>);

fn update_deltas(
    mut wheel_data: ResMut<CumulativeWheelData>,
    mut wheel_events: EventReader<MouseWheel>,
    prio: Res<InputPriorities>,
    mut prev: Local<InputPriorities>,
) {
    wheel_data.prev = wheel_data.current;
    wheel_data.current = None;
    for ev in wheel_events.read() {
        wheel_data.current = Some(wheel_data.current.unwrap_or_default() + Vec2::new(ev.x, ev.y));
    }
    if *prev != *prio {
        *prev = prio.clone();
        debug!("{prio:?}");
    }
}

fn handle_native_input(
    mut events: EventReader<SystemApi>,
    mut active: ResMut<CurrentNativeInputRequest>,
    mouse_input: Res<ButtonInput<MouseButton>>,
    key_input: Res<ButtonInput<KeyCode>>,
    mut priorities: ResMut<InputPriorities>,
) {
    if let Some(current) = active.0.take() {
        if let Some(key) = key_input.get_just_pressed().next() {
            current.send(InputIdentifier::Key(*key));
            priorities.release(InputType::All, InputPriority::BindInput);
        } else if let Some(mouse) = mouse_input.get_just_pressed().next() {
            current.send(InputIdentifier::Mouse(*mouse));
            priorities.release(InputType::All, InputPriority::BindInput);
        } else {
            active.0 = Some(current);
        }
        return;
    }

    if let Some(sender) = events
        .read()
        .filter_map(|e| {
            if let SystemApi::GetNativeInput(sender) = e {
                Some(sender)
            } else {
                None
            }
        })
        .last()
    {
        priorities.reserve(InputType::All, InputPriority::BindInput);
        active.0 = Some(sender.clone());
    }
}

fn handle_get_bindings(mut events: EventReader<SystemApi>, map: Res<InputMap>) {
    for sender in events.read().filter_map(|e| {
        if let SystemApi::GetBindings(sender) = e {
            Some(sender)
        } else {
            None
        }
    }) {
        sender.send(BindingsData {
            bindings: map.inputs.clone(),
        });
    }
}

fn handle_set_bindings(mut events: EventReader<SystemApi>, mut map: ResMut<InputMap>) {
    for (binding_data, sender) in events.read().filter_map(|e| {
        if let SystemApi::SetBindings(binding_data, sender) = e {
            Some((binding_data, sender))
        } else {
            None
        }
    }) {
        map.inputs = binding_data.bindings.clone();
        sender.send(());
    }
}
