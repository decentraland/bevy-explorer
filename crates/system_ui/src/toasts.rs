use bevy::{prelude::*, utils::HashMap};
use common::sets::SetupSets;
use scene_runner::Toasts;
use ui_core::{ui_builder::SpawnSpacer, BODY_TEXT_STYLE};

pub struct ToastsPlugin;

impl Plugin for ToastsPlugin {
    fn build(&self, app: &mut App) {
        app.add_startup_system(setup.in_set(SetupSets::Main));
        app.add_system(update_toasts);
    }
}

#[derive(Component)]
pub struct ToastMarker;
fn setup(mut commands: Commands) {
    commands
        .spawn(NodeBundle {
            style: Style {
                position_type: PositionType::Absolute,
                position: UiRect {
                    left: Val::Percent(30.0),
                    right: Val::Percent(30.0),
                    top: Val::Percent(10.0),
                    bottom: Val::Undefined,
                },
                max_size: Size::width(Val::Percent(40.0)),
                ..Default::default()
            },
            z_index: ZIndex::Global(1),
            ..Default::default()
        })
        .with_children(|c| {
            c.spacer();
            c.spawn((
                NodeBundle {
                    style: Style {
                        flex_direction: FlexDirection::Column,
                        ..Default::default()
                    },
                    background_color: Color::rgba(0.2, 0.2, 1.0, 0.2).into(),
                    ..Default::default()
                },
                ToastMarker,
            ));
            c.spacer();
        });
}

fn update_toasts(
    mut commands: Commands,
    toast_display: Query<Entity, With<ToastMarker>>,
    toasts: Res<Toasts>,
    time: Res<Time>,
    mut displays: Local<HashMap<&'static str, Option<Entity>>>,
) {
    let Ok(toaster_ent) = toast_display.get_single() else {
        return;
    };

    let mut prev_displays = std::mem::take(&mut *displays);

    for (key, toast) in &toasts.0 {
        let Some(maybe_ent) = prev_displays.remove(key) else {
            commands.entity(toaster_ent).with_children(|c| {
                let id = c.spawn(TextBundle {
                    text: Text::from_section(&toast.message, BODY_TEXT_STYLE.get().unwrap().clone()).with_alignment(TextAlignment::Center),
                    background_color: Color::rgba(0.2, 0.2, 1.0, 0.0).into(),
                    ..Default::default()
                }).id();
                displays.insert(key, Some(id));
            });
            continue;
        };

        if let Some(ent) = maybe_ent {
            if toast.time < time.elapsed_seconds() - 5.0 {
                commands.entity(ent).despawn_recursive();
                displays.insert(key, None);
                continue;
            }
        }

        displays.insert(key, maybe_ent);
    }

    for (key, ent) in prev_displays {
        if !displays.contains_key(key) {
            if let Some(ent) = ent {
                commands.entity(ent).despawn_recursive();
            }
        }
    }
}
