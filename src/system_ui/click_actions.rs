// allow on_click handlers defined as systems or closures on buttons
// commands.spawn((ButtonBundle::default(), click_actions::on_click(|| println!("clicked"))));
pub struct UiActionPlugin;
use bevy::{ecs::system::BoxedSystem, prelude::*, utils::HashSet};

use super::focus::Focus;

impl Plugin for UiActionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UiActions>().add_system(run_ui_actions);
    }
}

#[derive(Component)]
pub struct ClickAction(usize);
#[derive(Component)]
pub struct HoverEnterAction(usize);
#[derive(Component)]
pub struct HoverExitAction(usize);
#[derive(Component)]
pub struct FocusAction(usize);
#[derive(Component)]
pub struct DefocusAction(usize);

struct Action {
    system: BoxedSystem,
    initialized: bool,
    run_already: bool,
}

impl Action {
    fn new<M>(system: impl IntoSystem<(), (), M>) -> Self {
        Self {
            system: Box::new(IntoSystem::into_system(system)),
            initialized: false,
            run_already: false,
        }
    }
}

#[derive(Resource, Default)]
pub struct UiActions {
    click: Vec<Action>,
    hover_enter: Vec<Action>,
    hover_exit: Vec<Action>,
    focus: Vec<Action>,
    defocus: Vec<Action>,
}

impl UiActions {
    pub fn on_click<M>(&mut self, system: impl IntoSystem<(), (), M>) -> ClickAction {
        self.click.push(Action::new(system));
        ClickAction(self.click.len() - 1)
    }

    pub fn on_hover_enter<M>(&mut self, system: impl IntoSystem<(), (), M>) -> HoverEnterAction {
        self.hover_enter.push(Action::new(system));
        HoverEnterAction(self.hover_enter.len() - 1)
    }

    pub fn on_hover_exit<M>(&mut self, system: impl IntoSystem<(), (), M>) -> HoverExitAction {
        self.hover_exit.push(Action::new(system));
        HoverExitAction(self.hover_exit.len() - 1)
    }

    pub fn on_focus<M>(&mut self, system: impl IntoSystem<(), (), M>) -> FocusAction {
        self.focus.push(Action::new(system));
        FocusAction(self.focus.len() - 1)
    }

    pub fn on_defocus<M>(&mut self, system: impl IntoSystem<(), (), M>) -> DefocusAction {
        self.defocus.push(Action::new(system));
        DefocusAction(self.defocus.len() - 1)
    }
}

pub fn run_ui_actions(world: &mut World) {
    let click_list: HashSet<usize> = world
        .query::<(&ClickAction, &Interaction)>()
        .iter(world)
        .filter_map(|(action, interaction)| {
            matches!(interaction, Interaction::Clicked).then_some(action.0)
        })
        .collect();

    let hover_enter_list = world
        .query_filtered::<(&HoverEnterAction, &Interaction), Without<Focus>>()
        .iter(world)
        .filter_map(|(action, interaction)| {
            (!matches!(interaction, Interaction::None)).then_some(action.0)
        })
        .collect();

    let hover_exit_list = world
        .query_filtered::<(&HoverExitAction, &Interaction), Without<Focus>>()
        .iter(world)
        .filter_map(|(action, interaction)| {
            matches!(interaction, Interaction::None).then_some(action.0)
        })
        .collect();

    let focus_list = world
        .query_filtered::<&FocusAction, With<Focus>>()
        .iter(world)
        .map(|action| action.0)
        .collect();

    let defocus_list = world
        .query_filtered::<&DefocusAction, Without<Focus>>()
        .iter(world)
        .map(|action| action.0)
        .collect();

    world.resource_scope(|world: &mut World, mut ui_actions: Mut<UiActions>| {
        let UiActions {
            ref mut click,
            ref mut hover_enter,
            ref mut hover_exit,
            ref mut focus,
            ref mut defocus,
        } = *ui_actions;

        for (active_set, actions) in [
            (hover_enter_list, hover_enter),
            (click_list, click),
            (hover_exit_list, hover_exit),
            (focus_list, focus),
            (defocus_list, defocus),
        ] {
            for (ix, action) in actions.iter_mut().enumerate() {
                let active = active_set.contains(&ix);
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
        }
    });
}
