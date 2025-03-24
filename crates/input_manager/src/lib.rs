// input settings

use std::collections::BTreeSet;

use bevy::{
    ecs::system::SystemParam,
    input::mouse::{MouseMotion, MouseWheel},
    prelude::*,
    utils::HashMap,
    window::PrimaryWindow,
};

use common::{rpc::RpcResultSender, structs::CursorLocks};
use dcl_component::proto_components::sdk::components::common::InputAction;
pub use system_bridge::{Action, SystemAction, MOVE_SET, POINTER_SET, SCROLL_SET};
use system_bridge::{
    AxisIdentifier, BindingsData, InputDirection, InputDirectionalSet, InputIdentifier, SystemApi,
};

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
        app.insert_resource(CumulativeAxisData {
            multipliers: HashMap::from_iter([
                (AxisIdentifier::GamepadRight, 10.0),
                (AxisIdentifier::MouseWheel, 10.0),
            ]),
            ..Default::default()
        });
        app.add_systems(
            PreUpdate,
            (
                update_deltas,
                handle_native_input,
                handle_get_bindings,
                handle_set_bindings,
                handle_pointer_motion,
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
                    vec![
                        InputIdentifier::Mouse(MouseButton::Left),
                        InputIdentifier::Gamepad(GamepadButtonType::LeftTrigger2),
                    ],
                ),
                (
                    Action::Scene(InputAction::IaPrimary),
                    vec![
                        InputIdentifier::Key(KeyCode::KeyE),
                        InputIdentifier::Gamepad(GamepadButtonType::South),
                    ],
                ),
                (
                    Action::Scene(InputAction::IaSecondary),
                    vec![
                        InputIdentifier::Key(KeyCode::KeyF),
                        InputIdentifier::Gamepad(GamepadButtonType::East),
                    ],
                ),
                (
                    Action::Scene(InputAction::IaForward),
                    vec![
                        InputIdentifier::Key(KeyCode::KeyW),
                        InputIdentifier::Analog(AxisIdentifier::GamepadLeft, InputDirection::Up),
                    ],
                ),
                (
                    Action::Scene(InputAction::IaBackward),
                    vec![
                        InputIdentifier::Key(KeyCode::KeyS),
                        InputIdentifier::Analog(AxisIdentifier::GamepadLeft, InputDirection::Down),
                    ],
                ),
                (
                    Action::Scene(InputAction::IaRight),
                    vec![
                        InputIdentifier::Key(KeyCode::KeyD),
                        InputIdentifier::Analog(AxisIdentifier::GamepadLeft, InputDirection::Right),
                    ],
                ),
                (
                    Action::Scene(InputAction::IaLeft),
                    vec![
                        InputIdentifier::Key(KeyCode::KeyA),
                        InputIdentifier::Analog(AxisIdentifier::GamepadLeft, InputDirection::Left),
                    ],
                ),
                (
                    Action::Scene(InputAction::IaJump),
                    vec![
                        InputIdentifier::Key(KeyCode::Space),
                        InputIdentifier::Gamepad(GamepadButtonType::North),
                    ],
                ),
                (
                    Action::Scene(InputAction::IaWalk),
                    vec![InputIdentifier::Key(KeyCode::ShiftLeft)],
                ),
                (
                    Action::Scene(InputAction::IaAction3),
                    vec![
                        InputIdentifier::Key(KeyCode::Digit1),
                        InputIdentifier::Gamepad(GamepadButtonType::DPadUp),
                    ],
                ),
                (
                    Action::Scene(InputAction::IaAction4),
                    vec![
                        InputIdentifier::Key(KeyCode::Digit2),
                        InputIdentifier::Gamepad(GamepadButtonType::DPadRight),
                    ],
                ),
                (
                    Action::Scene(InputAction::IaAction5),
                    vec![
                        InputIdentifier::Key(KeyCode::Digit3),
                        InputIdentifier::Gamepad(GamepadButtonType::DPadDown),
                    ],
                ),
                (
                    Action::Scene(InputAction::IaAction6),
                    vec![
                        InputIdentifier::Key(KeyCode::Digit4),
                        InputIdentifier::Gamepad(GamepadButtonType::DPadLeft),
                    ],
                ),
                (
                    Action::System(SystemAction::CameraLock),
                    vec![
                        InputIdentifier::Mouse(MouseButton::Right),
                        InputIdentifier::Gamepad(GamepadButtonType::RightTrigger2),
                    ],
                ),
                (
                    Action::System(SystemAction::Emote),
                    vec![InputIdentifier::Key(KeyCode::AltLeft)],
                ),
                (
                    Action::System(SystemAction::Cancel),
                    vec![
                        InputIdentifier::Key(KeyCode::Escape),
                        InputIdentifier::Gamepad(GamepadButtonType::Select),
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
                        InputIdentifier::Gamepad(GamepadButtonType::LeftTrigger),
                    ],
                ),
                (
                    Action::System(SystemAction::CameraZoomOut),
                    vec![
                        InputIdentifier::Analog(AxisIdentifier::MouseWheel, InputDirection::Down),
                        InputIdentifier::Gamepad(GamepadButtonType::RightTrigger),
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
                        InputIdentifier::Gamepad(GamepadButtonType::North),
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
pub struct CumulativeAxisData {
    raw_mouse: Vec2,
    current: HashMap<AxisIdentifier, Vec2>,
    prev: HashMap<AxisIdentifier, Vec2>,
    multipliers: HashMap<AxisIdentifier, f32>,
}

impl CumulativeAxisData {
    fn _analog(vec: Option<&Vec2>, dir: InputDirection) -> f32 {
        let vec = vec.copied().unwrap_or_default();
        match dir {
            InputDirection::Up => vec.y.max(0.0),
            InputDirection::Down => -vec.y.min(0.0),
            InputDirection::Left => -vec.x.min(0.0),
            InputDirection::Right => vec.x.max(0.0),
        }
    }

    fn check_dir(vec: Option<&Vec2>, dir: InputDirection) -> bool {
        Self::_analog(vec, dir) > 0.0
    }

    pub fn just_down(&self, ident: AxisIdentifier, dir: InputDirection) -> bool {
        Self::check_dir(self.current.get(&ident), dir)
            && !Self::check_dir(self.prev.get(&ident), dir)
    }

    pub fn just_up(&self, ident: AxisIdentifier, dir: InputDirection) -> bool {
        !Self::check_dir(self.current.get(&ident), dir)
            && Self::check_dir(self.prev.get(&ident), dir)
    }

    pub fn down(&self, ident: AxisIdentifier, dir: InputDirection) -> bool {
        Self::check_dir(self.current.get(&ident), dir)
    }

    pub fn analog(&self, ident: AxisIdentifier, dir: InputDirection) -> f32 {
        Self::_analog(self.current.get(&ident), dir) * self.multipliers.get(&ident).unwrap_or(&1.0)
    }
}

#[derive(SystemParam)]
pub struct InputManager<'w> {
    map: Res<'w, InputMap>,
    mouse_input: Res<'w, ButtonInput<MouseButton>>,
    key_input: Res<'w, ButtonInput<KeyCode>>,
    axis_data: ResMut<'w, CumulativeAxisData>,
    gamepad_input: Res<'w, ButtonInput<GamepadButton>>,
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
            InputIdentifier::Gamepad(b) => {
                self.gamepad_input
                    .get_just_pressed()
                    .any(|p| &p.button_type == b)
                    && self.check_priority(item, priority)
            }
            InputIdentifier::Analog(axis, input_direction) => {
                self.axis_data.just_down(*axis, *input_direction)
                    && self.check_priority(item, priority)
            }
        })
    }

    pub fn just_up<T: Into<Action>>(&self, action: T) -> bool {
        self.inputs(action.into()).any(|item| match item {
            InputIdentifier::Key(k) => self.key_input.just_released(*k),
            InputIdentifier::Mouse(mb) => self.mouse_input.just_released(*mb),
            InputIdentifier::Gamepad(b) => self
                .gamepad_input
                .get_just_released()
                .any(|p| &p.button_type == b),
            InputIdentifier::Analog(axis, input_direction) => {
                self.axis_data.just_up(*axis, *input_direction)
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
            InputIdentifier::Gamepad(b) => {
                self.gamepad_input
                    .get_pressed()
                    .any(|p| &p.button_type == b)
                    && self.check_priority(item, priority)
            }
            InputIdentifier::Analog(axis, input_direction) => {
                self.axis_data.down(*axis, *input_direction) && self.check_priority(item, priority)
            }
        })
    }

    pub fn get_analog(&self, set: InputDirectionalSet, priority: InputPriority) -> Vec2 {
        let mut amts = set.0.iter().map(|a| {
            self.inputs(*a)
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
                    InputIdentifier::Gamepad(b) => {
                        if self
                            .gamepad_input
                            .get_pressed()
                            .any(|p| &p.button_type == b)
                            && self.check_priority(item, priority)
                        {
                            1.0
                        } else {
                            0.0
                        }
                    }
                    InputIdentifier::Analog(axis, input_direction) => {
                        let analog = self.axis_data.analog(*axis, *input_direction);
                        if analog > 0.0 && self.check_priority(item, priority) {
                            analog
                        } else {
                            0.0
                        }
                    }
                })
                .sum::<f32>()
        });

        let mouse = if set == POINTER_SET {
            self.axis_data.raw_mouse
        } else {
            Vec2::ZERO
        };

        mouse
            + Vec2::new(
                amts.next().unwrap() - amts.next().unwrap(),
                amts.next().unwrap() - amts.next().unwrap(),
            )
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
                    InputIdentifier::Gamepad(b) => {
                        self.gamepad_input
                            .get_just_pressed()
                            .any(|p| &p.button_type == b)
                            && self.check_priority(button, InputPriority::Scene)
                    }
                    InputIdentifier::Analog(axis, input_direction) => {
                        self.axis_data.just_down(*axis, *input_direction)
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
                    InputIdentifier::Gamepad(b) => self
                        .gamepad_input
                        .get_just_released()
                        .any(|p| &p.button_type == b),
                    InputIdentifier::Analog(axis, input_direction) => {
                        self.axis_data.just_up(*axis, *input_direction)
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

struct CurrentNativeInputRequest {
    sender: RpcResultSender<InputIdentifier>,
    axes: HashMap<AxisIdentifier, Vec2>,
}

fn update_deltas(
    mut axis_data: ResMut<CumulativeAxisData>,
    mut wheel_events: EventReader<MouseWheel>,
    pad_axes: Res<Axis<GamepadAxis>>,
    prio: Res<InputPriorities>,
    mut prev: Local<InputPriorities>,
) {
    axis_data.prev = std::mem::take(&mut axis_data.current);
    for ev in wheel_events.read() {
        *axis_data
            .current
            .entry(AxisIdentifier::MouseWheel)
            .or_default() += Vec2::new(ev.x, ev.y);
    }
    for device in pad_axes.devices() {
        if let Some(value) = pad_axes.get(*device) {
            match device.axis_type {
                GamepadAxisType::LeftStickX => {
                    *axis_data
                        .current
                        .entry(AxisIdentifier::GamepadLeft)
                        .or_default() += Vec2::X * value
                }
                GamepadAxisType::LeftStickY => {
                    *axis_data
                        .current
                        .entry(AxisIdentifier::GamepadLeft)
                        .or_default() += Vec2::Y * value
                }
                GamepadAxisType::LeftZ => {
                    *axis_data
                        .current
                        .entry(AxisIdentifier::GamepadLeftTrigger)
                        .or_default() += Vec2::X * value
                }
                GamepadAxisType::RightStickX => {
                    *axis_data
                        .current
                        .entry(AxisIdentifier::GamepadRight)
                        .or_default() += Vec2::X * value
                }
                GamepadAxisType::RightStickY => {
                    *axis_data
                        .current
                        .entry(AxisIdentifier::GamepadRight)
                        .or_default() += Vec2::Y * value
                }
                GamepadAxisType::RightZ => {
                    *axis_data
                        .current
                        .entry(AxisIdentifier::GamepadRightTrigger)
                        .or_default() += Vec2::Y * value
                }
                GamepadAxisType::Other(_) => (),
            }
        }
    }
    if *prev != *prio {
        *prev = prio.clone();
        debug!("{prio:?}");
    }
}

fn handle_pointer_motion(
    locks: Res<CursorLocks>,
    mut input_manager: InputManager,
    mut window: Query<&mut Window, With<PrimaryWindow>>,
    mut mouse_events: EventReader<MouseMotion>,
    mut last_position: Local<Vec2>,
) {
    input_manager.axis_data.raw_mouse = Vec2::ZERO;
    let motion = input_manager.get_analog(POINTER_SET, InputPriority::BindInput);

    if let Ok(mut window) = window.get_single_mut() {
        let position = window.cursor_position().unwrap_or(*last_position);
        if window.cursor_position().is_some() {
            *last_position = position + motion;
        }

        if locks.0.is_empty() && motion != Vec2::ZERO {
            window.set_cursor_position(Some(position + motion));
        }
    }

    for ev in mouse_events.read() {
        input_manager.axis_data.raw_mouse += ev.delta;
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_native_input(
    mut events: EventReader<SystemApi>,
    mut active: Local<Option<CurrentNativeInputRequest>>,
    mouse_input: Res<ButtonInput<MouseButton>>,
    key_input: Res<ButtonInput<KeyCode>>,
    gamepad_input: Res<ButtonInput<GamepadButton>>,
    mut wheel_events: EventReader<MouseWheel>,
    mut mouse_events: EventReader<MouseMotion>,
    pad_axes: Res<Axis<GamepadAxis>>,
    mut priorities: ResMut<InputPriorities>,
) {
    fn vec2dir(vec: Vec2) -> InputDirection {
        let max = vec.abs().max_element();
        if max == vec.x {
            InputDirection::Right
        } else if max == vec.y {
            InputDirection::Up
        } else if max == -vec.x {
            InputDirection::Left
        } else {
            InputDirection::Down
        }
    }

    if let Some(mut current) = active.take() {
        if let Some(key) = key_input.get_just_pressed().next() {
            current.sender.send(InputIdentifier::Key(*key));
            return;
        } else if let Some(mouse) = mouse_input.get_just_pressed().next() {
            current.sender.send(InputIdentifier::Mouse(*mouse));
            return;
        } else if let Some(gamepad) = gamepad_input.get_just_pressed().next() {
            current
                .sender
                .send(InputIdentifier::Gamepad(gamepad.button_type));
            return;
        } else {
            for ev in mouse_events.read() {
                let axis = current.axes.entry(AxisIdentifier::MouseMove).or_default();
                *axis += ev.delta;
                if axis.abs().max_element() > 10.0 {
                    current.sender.send(InputIdentifier::Analog(
                        AxisIdentifier::MouseMove,
                        vec2dir(*axis),
                    ));
                    return;
                }
            }
            if let Some(ev) = wheel_events.read().next() {
                current.sender.send(InputIdentifier::Analog(
                    AxisIdentifier::MouseWheel,
                    vec2dir(Vec2::new(ev.x, ev.y)),
                ));
                return;
            }
            for device in pad_axes.devices() {
                if let Some(value) = pad_axes.get(*device) {
                    let (axis, value) = match device.axis_type {
                        GamepadAxisType::LeftStickX => {
                            (AxisIdentifier::GamepadLeft, Vec2::X * value)
                        }
                        GamepadAxisType::LeftStickY => {
                            (AxisIdentifier::GamepadLeft, Vec2::Y * value)
                        }
                        GamepadAxisType::LeftZ => {
                            (AxisIdentifier::GamepadLeftTrigger, Vec2::X * value)
                        }
                        GamepadAxisType::RightStickX => {
                            (AxisIdentifier::GamepadRight, Vec2::X * value)
                        }
                        GamepadAxisType::RightStickY => {
                            (AxisIdentifier::GamepadRight, Vec2::Y * value)
                        }
                        GamepadAxisType::RightZ => {
                            (AxisIdentifier::GamepadRightTrigger, Vec2::Y * value)
                        }
                        GamepadAxisType::Other(_) => continue,
                    };
                    let axis_val = current.axes.entry(axis).or_default();
                    *axis_val += value;
                    if axis_val.abs().max_element() > 10.0 {
                        current
                            .sender
                            .send(InputIdentifier::Analog(axis, vec2dir(*axis_val)));
                        return;
                    }
                }
            }

            *active = Some(current);
        }
        return;
    }

    mouse_events.clear();
    wheel_events.clear();
    priorities.release(InputType::All, InputPriority::BindInput);

    if let Some(sender) = events
        .read()
        .filter_map(|e| {
            if let SystemApi::GetNativeInput(sender) = e {
                Some(sender.clone())
            } else {
                None
            }
        })
        .last()
    {
        *active = Some(CurrentNativeInputRequest {
            sender,
            axes: Default::default(),
        });
        priorities.reserve(InputType::All, InputPriority::BindInput);
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
