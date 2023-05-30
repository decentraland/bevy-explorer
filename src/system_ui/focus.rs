use bevy::prelude::*;

use super::click_actions::run_ui_actions;

#[derive(Component)]
pub struct Focus;

#[derive(Component)]
pub struct Focusable;

pub struct FocusPlugin;

impl Plugin for FocusPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            (apply_system_buffers, defocus, focus)
                .chain()
                .after(run_ui_actions),
        );
    }
}

fn defocus(
    mut commands: Commands,
    focus_elements: Query<(Entity, Changed<Focus>), With<Focus>>,
    mouse_button_input: Res<Input<MouseButton>>,
) {
    let refocussed = mouse_button_input.any_just_pressed([MouseButton::Left, MouseButton::Right])
        || focus_elements.iter().any(|(_, added)| added);

    if refocussed {
        for (entity, added) in focus_elements.iter() {
            if !added {
                commands.entity(entity).remove::<Focus>();
            }
        }
    }
}

#[allow(clippy::type_complexity)]
fn focus(
    mut commands: Commands,
    focused_elements: Query<(Entity, &Interaction), (Changed<Interaction>, With<Focusable>)>,
) {
    for (entity, _) in focused_elements
        .iter()
        .filter(|(_, interaction)| matches!(interaction, Interaction::Clicked))
    {
        commands.entity(entity).insert(Focus);
    }
}
