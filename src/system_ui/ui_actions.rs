// allow on_click handlers defined as systems or closures on buttons
// commands.spawn((ButtonBundle::default(), click_actions::on_click(|| println!("clicked"))));
pub struct UiActionPlugin;
use std::marker::PhantomData;

use bevy::{
    ecs::{
        query::{ReadOnlyWorldQuery, WorldQuery},
        system::BoxedSystem,
    },
    prelude::*,
    utils::HashSet,
};

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
            .add_systems(
                (
                    gather_actions::<HoverEnter>,
                    gather_actions::<Click>,
                    gather_actions::<HoverExit>,
                    gather_actions::<Focus>,
                    gather_actions::<Defocus>,
                    gather_actions::<DataChanged>,
                    run_actions::<HoverEnter>,
                    run_actions::<Click>,
                    run_actions::<HoverExit>,
                    run_actions::<Focus>,
                    run_actions::<Defocus>,
                    run_actions::<DataChanged>,
                )
                    .chain()
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
}

pub struct Click;
impl ActionMarker for Click {
    type Component = &'static Interaction;
    fn activate(param: <Self::Component as WorldQuery>::Item<'_>) -> bool {
        matches!(param, Interaction::Clicked)
    }
}

pub struct HoverEnter;
impl ActionMarker for HoverEnter {
    type Component = &'static Interaction;
    fn activate(param: <Self::Component as WorldQuery>::Item<'_>) -> bool {
        matches!(param, Interaction::Hovered)
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
    type Component = Changed<DataChanged>;
    fn activate(param: <Self::Component as WorldQuery>::Item<'_>) -> bool {
        param
    }
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
            .insert(ActionIndex::<M>(ui_actions.0.len(), Default::default()));
        ui_actions.0.push(action.0.take().unwrap());
    }
}

pub fn run_actions<M: ActionMarker>(world: &mut World) {
    let action_list: HashSet<usize> = world
        .query::<(&ActionIndex<M>, M::Component)>()
        .iter(world)
        .filter_map(|(action, param)| M::activate(param).then_some(action.0))
        .collect();

    world.resource_scope(|world: &mut World, mut ui_actions: Mut<UiActions<M>>| {
        for (ix, action) in ui_actions.0.iter_mut().enumerate() {
            let active = action_list.contains(&ix);
            if active && !action.run_already {
                if !action.initialized {
                    action.system.initialize(world);
                    action.initialized = true;
                }
                action.system.run((), world);
                action.system.apply_buffers(world);
            }
            action.run_already = active;
        }
    });

    // TODO: cleanup removed actions
    // - add arc and sender to each action, notify on drop
    // - receiver in actions struct, record dropped
    // - allocate new from dropped list first
}
