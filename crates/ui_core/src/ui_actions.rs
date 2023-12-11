// allow on_click handlers defined as systems or closures on buttons
// commands.spawn((ButtonBundle::default(), click_actions::on_click(|| println!("clicked"))));
pub struct UiActionPlugin;
use std::marker::PhantomData;

use bevy::{
    ecs::{
        query::{ReadOnlyWorldQuery, WorldQuery},
        system::BoxedSystem,
    },
    input::mouse::MouseMotion,
    prelude::*,
    utils::{HashMap, HashSet},
};

use common::sets::SceneSets;

use super::focus::Focus;

#[derive(SystemSet, Debug, PartialEq, Eq, Hash, Clone)]
pub struct UiActionSet;

impl Plugin for UiActionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UiActions<HoverEnter>>()
            .init_resource::<UiActions<Click>>()
            .init_resource::<UiActions<HoverExit>>()
            .init_resource::<UiActions<Focus>>()
            .init_resource::<UiActions<Defocus>>()
            .init_resource::<UiActions<DataChanged>>()
            .init_resource::<UiActions<Dragged>>()
            .add_systems(
                Update,
                (
                    update_drag,
                    gather_actions::<HoverEnter>,
                    gather_actions::<Click>,
                    gather_actions::<HoverExit>,
                    gather_actions::<Focus>,
                    gather_actions::<Defocus>,
                    gather_actions::<DataChanged>,
                    gather_actions::<Dragged>,
                    apply_deferred,
                    run_actions::<HoverEnter>,
                    run_actions::<Click>,
                    run_actions::<HoverExit>,
                    run_actions::<Focus>,
                    run_actions::<Defocus>,
                    run_actions::<DataChanged>,
                    run_actions::<Dragged>,
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
}

pub trait ActionMarker: Send + Sync + 'static {
    type Component: ReadOnlyWorldQuery;

    fn activate(param: <Self::Component as WorldQuery>::Item<'_>) -> bool;
    fn repeat_activate() -> bool {
        false
    }
}

pub struct Click;
impl ActionMarker for Click {
    type Component = &'static Interaction;
    fn activate(param: <Self::Component as WorldQuery>::Item<'_>) -> bool {
        matches!(param, Interaction::Pressed)
    }
}

pub struct HoverEnter;
impl ActionMarker for HoverEnter {
    type Component = &'static Interaction;
    fn activate(param: <Self::Component as WorldQuery>::Item<'_>) -> bool {
        !matches!(param, Interaction::None)
    }
}
pub struct HoverExit;
impl ActionMarker for HoverExit {
    type Component = &'static Interaction;
    fn activate(param: <Self::Component as WorldQuery>::Item<'_>) -> bool {
        matches!(param, Interaction::None)
    }
}
impl ActionMarker for Focus {
    type Component = Option<&'static Focus>;
    fn activate(param: <Self::Component as WorldQuery>::Item<'_>) -> bool {
        param.is_some()
    }
}
pub struct Defocus;
impl ActionMarker for Defocus {
    type Component = Option<&'static Focus>;
    fn activate(param: <Self::Component as WorldQuery>::Item<'_>) -> bool {
        param.is_none()
    }
}

#[derive(Component)]
pub struct DataChanged;
impl ActionMarker for DataChanged {
    type Component = Option<Changed<DataChanged>>;
    fn activate(param: <Self::Component as WorldQuery>::Item<'_>) -> bool {
        param.unwrap_or(false)
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
    pub delta: Vec2,
}

#[derive(Component)]
struct ActionIndex<M: ActionMarker>(usize, PhantomData<M>);

struct ActionImpl {
    system: BoxedSystem,
    initialized: bool,
    run_already: bool,
}

impl ActionImpl {
    fn new<M>(system: impl IntoSystem<(), (), M>) -> Self {
        Self {
            system: Box::new(IntoSystem::into_system(system)),
            initialized: false,
            run_already: false,
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
        ui_actions.0.push(action.0.take().unwrap());
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
                action.system.run((), world);
                action.system.apply_deferred(world);
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
    mut q: Query<(Entity, &Interaction, Option<&mut DragData>), With<ActionIndex<Dragged>>>,
    mut mouse_events: EventReader<MouseMotion>,
) {
    let delta: Vec2 = mouse_events.read().map(|mme| mme.delta).sum();

    for (ent, interaction, drag_data) in q.iter_mut() {
        let Some(mut drag_data) = drag_data else {
            commands.entity(ent).try_insert(DragData::default());
            continue;
        };

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
        drag_data.delta = delta;
    }
}
