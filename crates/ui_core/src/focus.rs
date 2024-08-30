use bevy::prelude::*;

use crate::ui_actions::UiFocusActionSet;

use super::ui_actions::UiActionSet;
use common::sets::SceneSets;

#[derive(Component)]
pub struct Focus;

#[derive(Component)]
pub struct Focusable;

// focus + blockkeyboard -> input manager will block normal key ops
#[derive(Component)]
pub struct BlockKeyboard;

pub struct FocusPlugin;

impl Plugin for FocusPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PreUpdate,
            (apply_deferred, defocus, focus)
                .chain()
                .in_set(SceneSets::UiActions)
                .after(UiActionSet)
                .before(UiFocusActionSet),
        );
    }
}

fn defocus(
    mut commands: Commands,
    focus_elements: Query<(Entity, Ref<Focus>)>,
    mouse_button_input: Res<ButtonInput<MouseButton>>,
) {
    let refocussed = mouse_button_input.any_just_pressed([MouseButton::Left, MouseButton::Right])
        || focus_elements.iter().any(|(_, focus)| focus.is_changed());

    if refocussed {
        for (entity, ref_focus) in focus_elements.iter() {
            if !ref_focus.is_changed() {
                commands.entity(entity).remove::<Focus>();
                debug!("defocus {:?}", entity);
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
        .filter(|(_, interaction)| matches!(interaction, Interaction::Pressed))
    {
        commands.entity(entity).try_insert(Focus);
        debug!("focus {:?}", entity);
    }
}
