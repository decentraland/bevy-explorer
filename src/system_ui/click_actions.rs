// allow on_click handlers defined as systems or closures on buttons
// commands.spawn((ButtonBundle::default(), click_actions::on_click(|| println!("clicked"))));
pub struct ClickActionPlugin;
use bevy::{ecs::system::BoxedSystem, prelude::*, utils::HashSet};

impl Plugin for ClickActionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ClickActions>()
            .add_system(run_click_actions);
    }
}

#[derive(Component)]
pub struct ClickAction(usize);

#[derive(Resource, Default)]
pub struct ClickActions {
    new: Vec<BoxedSystem>,
    actions: Vec<(BoxedSystem, bool)>,
}

impl ClickActions {
    pub fn on_click<M>(&mut self, system: impl IntoSystem<(), (), M>) -> ClickAction {
        self.new.push(Box::new(IntoSystem::into_system(system)));
        ClickAction(self.new.len() + self.actions.len() - 1)
    }
}

fn run_click_actions(world: &mut World) {
    let mut q = world.query::<(&ClickAction, &Interaction)>();
    let clicked: HashSet<usize> = q
        .iter(world)
        .filter_map(|(action, interaction)| {
            matches!(interaction, Interaction::Clicked).then_some(action.0)
        })
        .collect();

    world.resource_scope(|world: &mut World, mut actions: Mut<ClickActions>| {
        let mut new = std::mem::take(&mut actions.new);
        actions.actions.extend(new.drain(..).map(|mut system| {
            system.initialize(world);
            (system, false)
        }));

        for (ix, (ref mut system, ref mut clicked_already)) in
            actions.actions.iter_mut().enumerate()
        {
            let clicked = clicked.contains(&ix);
            if clicked && !*clicked_already {
                system.run((), world);
            }
            *clicked_already = clicked;
        }
    });
}
