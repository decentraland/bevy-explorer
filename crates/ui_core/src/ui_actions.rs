// allow on_click handlers defined as systems or closures on buttons
// commands.spawn((ButtonBundle::default(), click_actions::on_click(|| println!("clicked"))));
pub struct UiActionPlugin;
use std::marker::PhantomData;

use bevy::{
    ecs::{
        query::{QueryData, WorldQuery},
        system::BoxedSystem,
    },
    input::mouse::{MouseMotion, MouseWheel},
    prelude::*,
    utils::{HashMap, HashSet},
    window::PrimaryWindow,
};

use common::sets::SceneSets;

use super::focus::Focus;

#[derive(Component)]
pub struct Enabled(pub bool);

#[derive(SystemSet, Debug, PartialEq, Eq, Hash, Clone)]
pub struct UiActionSet;

#[derive(Resource, Deref)]
pub struct UiCaller(pub Entity);

impl Plugin for UiActionPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(UiCaller(Entity::PLACEHOLDER))
            .init_resource::<UiActions<HoverEnter>>()
            .init_resource::<UiActions<Click>>()
            .init_resource::<UiActions<HoverExit>>()
            .init_resource::<UiActions<Focus>>()
            .init_resource::<UiActions<Defocus>>()
            .init_resource::<UiActions<DataChanged>>()
            .init_resource::<UiActions<Dragged>>()
            .init_resource::<UiActions<ClickNoDrag>>()
            .init_resource::<UiActions<MouseWheeled>>()
            .add_systems(
                Update,
                (
                    update_drag,
                    update_wheel,
                    (
                        gather_actions::<HoverEnter>,
                        gather_actions::<Click>,
                        gather_actions::<HoverExit>,
                        gather_actions::<Focus>,
                        gather_actions::<Defocus>,
                        gather_actions::<DataChanged>,
                        gather_actions::<Dragged>,
                        gather_actions::<ClickNoDrag>,
                        gather_actions::<MouseWheeled>,
                    )
                        .chain(),
                    apply_deferred,
                    (
                        run_actions::<HoverEnter>,
                        run_actions::<Click>,
                        run_actions::<HoverExit>,
                        run_actions::<Focus>,
                        run_actions::<Defocus>,
                        run_actions::<DataChanged>,
                        run_actions::<Dragged>,
                        run_actions::<ClickNoDrag>,
                        run_actions::<MouseWheeled>,
                    )
                        .chain(),
                )
                    .chain()
                    .in_set(SceneSets::UiActions)
                    .in_set(UiActionSet),
            );
    }
}

#[derive(Component)]
pub struct On<M: ActionMarker>(Option<ActionImpl>, PhantomData<M>);

impl<M: ActionMarker> On<M> {
    pub fn new<S>(system: impl IntoSystem<(), (), S>) -> Self {
        Self(Some(ActionImpl::new(system)), Default::default())
    }

    pub fn close_and<S>(system: impl IntoSystem<(), (), S>) -> Self {
        Self(
            Some(ActionImpl::new(close_ui.pipe(system))),
            Default::default(),
        )
    }
}

pub fn close_ui(mut commands: Commands, parents: Query<&Parent>, c: Res<UiCaller>) {
    let mut ent = c.0;
    while let Ok(p) = parents.get(ent) {
        ent = **p;
    }
    if let Some(commands) = commands.get_entity(ent) {
        commands.despawn_recursive();
    }
}

pub trait ActionMarker: Send + Sync + 'static {
    type Component: QueryData;

    fn activate(param: <<Self::Component as QueryData>::ReadOnly as WorldQuery>::Item<'_>) -> bool;
    fn repeat_activate() -> bool {
        false
    }
}

pub struct Click;
impl ActionMarker for Click {
    type Component = (&'static Interaction, Option<&'static Enabled>);
    fn activate((interact, enabled): <<Self::Component as QueryData>::ReadOnly as WorldQuery>::Item<'_>) -> bool {
        matches!(interact, Interaction::Pressed) && enabled.map_or(true, |a| a.0)
    }
}

pub struct HoverEnter;
impl ActionMarker for HoverEnter {
    type Component = (&'static Interaction, Option<&'static Enabled>);
    fn activate((interact, enabled): <<Self::Component as QueryData>::ReadOnly as WorldQuery>::Item<'_>) -> bool {
        !matches!(interact, Interaction::None) && enabled.map_or(true, |a| a.0)
    }
}
pub struct HoverExit;
impl ActionMarker for HoverExit {
    type Component = (&'static Interaction, Option<&'static Enabled>);
    fn activate((interact, enabled): <<Self::Component as QueryData>::ReadOnly as WorldQuery>::Item<'_>) -> bool {
        matches!(interact, Interaction::None) && enabled.map_or(true, |a| a.0)
    }
}
impl ActionMarker for Focus {
    type Component = Option<&'static Focus>;
    fn activate(param: <<Self::Component as QueryData>::ReadOnly as WorldQuery>::Item<'_>) -> bool {
        param.is_some()
    }
}
pub struct Defocus;
impl ActionMarker for Defocus {
    type Component = Option<&'static Focus>;
    fn activate(param: <<Self::Component as QueryData>::ReadOnly as WorldQuery>::Item<'_>) -> bool {
        param.is_none()
    }
}

#[derive(Component)]
pub struct DataChanged;
impl ActionMarker for DataChanged {
    type Component = Option<Ref<'static, DataChanged>>;
    fn activate(param: <<Self::Component as QueryData>::ReadOnly as WorldQuery>::Item<'_>) -> bool {
        param.map(|p| p.is_changed()).unwrap_or(false)
    }
}

#[derive(Component)]
pub struct Dragged;
impl ActionMarker for Dragged {
    type Component = Option<&'static DragData>;

    fn activate(param: Option<&DragData>) -> bool {
        param.map_or(false, |p| p.trigger)
    }

    fn repeat_activate() -> bool {
        true
    }
}

#[derive(Component, Default)]
pub struct DragData {
    was_pressed: bool,
    pub trigger: bool,
    pub delta_pixels: Vec2,
    pub delta_viewport: Vec2,
}

#[derive(Component)]
pub struct ClickNoDrag;
impl ActionMarker for ClickNoDrag {
    type Component = Option<&'static ClickNoDragData>;

    fn activate(param: Option<&ClickNoDragData>) -> bool {
        param.map_or(false, |p| p.trigger)
    }
}

#[derive(Component, Default)]
pub struct ClickNoDragData {
    pub was_pressed: bool,
    pub valid: bool,
    pub trigger: bool,
}

#[derive(Component)]
pub struct MouseWheeled;
impl ActionMarker for MouseWheeled {
    type Component = Option<&'static MouseWheelData>;

    fn activate(param: Option<&MouseWheelData>) -> bool {
        param.map_or(false, |p| p.wheel != 0.0)
    }

    fn repeat_activate() -> bool {
        true
    }
}

#[derive(Component, Default)]
pub struct MouseWheelData {
    pub wheel: f32,
}

#[derive(Component)]
struct ActionIndex<M: ActionMarker>(usize, PhantomData<M>);

struct ActionImpl {
    system: BoxedSystem,
    initialized: bool,
    run_already: bool,
    entity: Entity,
}

impl ActionImpl {
    fn new<M>(system: impl IntoSystem<(), (), M>) -> Self {
        Self {
            system: Box::new(IntoSystem::into_system(system)),
            initialized: false,
            run_already: false,
            entity: Entity::PLACEHOLDER,
        }
    }
}

#[derive(Resource)]
struct UiActions<M: ActionMarker>(Vec<ActionImpl>, PhantomData<M>);

impl<M: ActionMarker> Default for UiActions<M> {
    fn default() -> Self {
        Self(Default::default(), Default::default())
    }
}

fn gather_actions<M: ActionMarker>(
    mut commands: Commands,
    mut ui_actions: ResMut<UiActions<M>>,
    mut new_actions: Query<(Entity, &mut On<M>), Without<ActionIndex<M>>>,
) {
    for (ent, mut action) in new_actions.iter_mut() {
        commands
            .entity(ent)
            .try_insert(ActionIndex::<M>(ui_actions.0.len(), Default::default()));
        let mut action = action.0.take().unwrap();
        action.entity = ent;
        ui_actions.0.push(action);
    }
}

pub fn run_actions<M: ActionMarker>(world: &mut World) {
    let active_list: HashMap<usize, bool> = world
        .query::<(&ActionIndex<M>, M::Component)>()
        .iter(world)
        .map(|(action, param)| (action.0, M::activate(param)))
        .collect();

    let mut removed: HashSet<usize> = HashSet::default();
    world.resource_scope(|world: &mut World, mut ui_actions: Mut<UiActions<M>>| {
        let mut index = 0;

        ui_actions.0.retain_mut(|action| {
            let Some(active) = active_list.get(&index) else {
                removed.insert(index);
                index += 1;
                return false;
            };

            if *active && !action.run_already {
                if !action.initialized {
                    action.system.initialize(world);
                    action.initialized = true;
                }
                world.resource_mut::<UiCaller>().0 = action.entity;
                action.system.run((), world);
                action.system.apply_deferred(world);
                world.resource_mut::<UiCaller>().0 = Entity::PLACEHOLDER;
            }
            action.run_already = *active && !M::repeat_activate();

            index += 1;
            true
        })
    });

    if !removed.is_empty() {
        world
            .query::<&mut ActionIndex<M>>()
            .iter_mut(world)
            .for_each(|mut action_index| {
                action_index.0 -= removed.iter().filter(|&r| *r < action_index.0).count();
            });
    }
}

#[allow(clippy::type_complexity)]
fn update_drag(
    mut commands: Commands,
    mut q: Query<
        (
            Entity,
            &Interaction,
            Option<&mut DragData>,
            Option<&mut ClickNoDragData>,
        ),
        Or<(With<ActionIndex<Dragged>>, With<ActionIndex<ClickNoDrag>>)>,
    >,
    mut mouse_events: EventReader<MouseMotion>,
    window: Query<&Window, With<PrimaryWindow>>,
) {
    let delta: Vec2 = mouse_events.read().map(|mme| mme.delta).sum();

    for (ent, interaction, drag_data, click_no_drag_data) in q.iter_mut() {
        let (Some(mut drag_data), Some(mut click_no_drag_data)) = (drag_data, click_no_drag_data)
        else {
            commands
                .entity(ent)
                .try_insert((DragData::default(), ClickNoDragData::default()));
            continue;
        };

        if interaction == &Interaction::Pressed {
            click_no_drag_data.trigger = false;

            if !click_no_drag_data.was_pressed {
                click_no_drag_data.was_pressed = true;
                click_no_drag_data.valid = true;
            }

            if delta != Vec2::ZERO {
                click_no_drag_data.valid = false;
            }
        } else {
            if click_no_drag_data.was_pressed && click_no_drag_data.valid {
                click_no_drag_data.trigger = true;
            }
            click_no_drag_data.was_pressed = false;
            click_no_drag_data.valid = false;
        }

        if interaction != &Interaction::Pressed {
            drag_data.trigger = false;
            drag_data.was_pressed = false;
            continue;
        }

        if !drag_data.was_pressed {
            drag_data.was_pressed = true;
            continue;
        }

        drag_data.trigger = delta != Vec2::ZERO;
        drag_data.delta_pixels = delta;

        let Ok(window) = window.get_single() else {
            return;
        };
        drag_data.delta_viewport = delta / Vec2::new(window.width(), window.height());
    }
}

#[allow(clippy::type_complexity)]
fn update_wheel(
    mut commands: Commands,
    mut q: Query<(Entity, &Interaction, Option<&mut MouseWheelData>), With<ActionIndex<Dragged>>>,
    mut wheel_events: EventReader<MouseWheel>,
) {
    let delta: f32 = wheel_events.read().map(|we| we.y).sum();

    for (ent, interaction, wheel_data) in q.iter_mut() {
        let Some(mut wheel_data) = wheel_data else {
            commands.entity(ent).try_insert(MouseWheelData::default());
            continue;
        };

        if interaction == &Interaction::None {
            wheel_data.wheel = 0.0;
            continue;
        } else {
            wheel_data.wheel = delta;
        }
    }
}

pub trait EventDefaultExt {
    fn send_default_on<A: ActionMarker>() -> On<A>;
}

impl<E: Event + Default> EventDefaultExt for E {
    fn send_default_on<A: ActionMarker>() -> On<A> {
        On::<A>::new(|mut e: EventWriter<Self>| {
            e.send_default();
        })
    }
}

pub trait EventCloneExt {
    fn send_value_on<A: ActionMarker>(self) -> On<A>;
}

impl<E: Event + Clone> EventCloneExt for E {
    fn send_value_on<A: ActionMarker>(self) -> On<A> {
        On::<A>::new(move |mut e: EventWriter<Self>| {
            e.send(self.clone());
        })
    }
}

pub trait EntityActionExt {
    fn despawn_recursive_on<A: ActionMarker>(&self) -> On<A>;
    fn despawn_recursive_and_close_on<A: ActionMarker>(&self) -> On<A>;
}

impl EntityActionExt for Entity {
    fn despawn_recursive_on<A: ActionMarker>(&self) -> On<A> {
        let ent = *self;
        On::<A>::new(move |mut commands: Commands| {
            commands.entity(ent).despawn_recursive();
        })
    }

    fn despawn_recursive_and_close_on<A: ActionMarker>(&self) -> On<A> {
        let ent = *self;
        On::<A>::new(close_ui.pipe(move |mut commands: Commands| {
            commands.entity(ent).despawn_recursive();
        }))
    }
}
