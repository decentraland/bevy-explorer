use bevy::{platform::collections::HashMap, prelude::*};
use bevy_dui::{DuiCommandsExt, DuiProps, DuiRegistry};
use scene_runner::Toasts;

pub struct ToastsPlugin;

impl Plugin for ToastsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter::<ui_core::State>(ui_core::State::Ready), setup);
        app.add_systems(Update, update_toasts);
    }
}

#[derive(Component)]
pub struct ToastMarker;
fn setup(mut commands: Commands, dui: Res<DuiRegistry>) {
    let inner = commands
        .spawn_template(&dui, "toaster", DuiProps::default())
        .unwrap()
        .named("inner");
    commands.entity(inner).insert(ToastMarker);
}

fn update_toasts(
    mut commands: Commands,
    toast_display: Query<Entity, With<ToastMarker>>,
    mut toasts: ResMut<Toasts>,
    time: Res<Time>,
    mut displays: Local<HashMap<String, Option<Entity>>>,
    dui: Res<DuiRegistry>,
) {
    let Ok(toaster_ent) = toast_display.single() else {
        return;
    };

    let mut prev_displays = std::mem::take(&mut *displays);

    for (key, toast) in &mut toasts.0 {
        let Some(maybe_ent) = prev_displays.remove(key) else {
            let components = commands
                .entity(toaster_ent)
                .spawn_template(
                    &dui,
                    "toast",
                    DuiProps::new().with_prop("toast", toast.message.clone()),
                )
                .unwrap();
            displays.insert(key.clone(), Some(components.root));
            if let Some(on_click) = toast.on_click.take() {
                commands
                    .entity(components.root)
                    .insert((Interaction::default(), on_click));
            }
            continue;
        };

        if let Some(ent) = maybe_ent {
            if toast.time < time.elapsed_secs() - 5.0 {
                commands.entity(ent).despawn();
                displays.insert(key.clone(), None);
                continue;
            }
        }

        displays.insert(key.clone(), maybe_ent);
    }

    for (key, ent) in prev_displays {
        if !displays.contains_key(&key) {
            if let Some(ent) = ent {
                commands.entity(ent).despawn();
            }
        }
    }

    toasts
        .0
        .retain(|_, toast| toast.last_update > time.elapsed_secs() - 5.0);
}
