
use bevy::prelude::*;
use bevy_dui::*;

#[derive(Component)]
pub struct Spinner;

pub(crate) fn spin_spinners(mut q: Query<&mut Transform, With<Spinner>>, time: Res<Time>) {
    for mut t in q.iter_mut() {
        t.rotation = Quat::from_rotation_z(time.elapsed_seconds() * 2.22);
    }
}

pub struct DuiSpinnerTemplate;
impl DuiTemplate for DuiSpinnerTemplate {
    fn render(
        &self,
        commands: &mut bevy::ecs::system::EntityCommands,
        _: &mut DuiProps,
        dui_registry: &DuiRegistry,
    ) -> Result<bevy_dui::NodeMap, anyhow::Error> {
        commands.apply_template(dui_registry, "spinner-base", DuiProps::new())?;
        commands.insert(Spinner);
        Ok(Default::default())
    }
}
