use bevy::prelude::*;

use crate::ui_actions::UiFocusActionSet;

use super::ui_actions::UiActionSet;
use common::sets::SceneSets;

#[derive(Component)]
pub struct Focus;

// use when you need to add focus but it is being done programatically and should not count as a new "focus" event
// this allows a click to still defocus the target
#[derive(Component)]
pub struct FocusIsNotReallyNew;

#[derive(Component)]
pub struct Focusable;

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
    focus_elements: Query<(Entity, Ref<Focus>, Option<&FocusIsNotReallyNew>)>,
    mouse_button_input: Res<ButtonInput<MouseButton>>,
) {
    let refocussed = mouse_button_input.any_just_pressed([MouseButton::Left, MouseButton::Right])
        || focus_elements
            .iter()
            .any(|(_, focus, not_sticky)| focus.is_changed() && not_sticky.is_none());

    if refocussed {
        for (entity, ref_focus, not_sticky) in focus_elements.iter() {
            if !ref_focus.is_changed() || not_sticky.is_some() {
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
        .filter(|(_, interaction)| matches!(interaction, Interaction::Pressed))
    {
        commands.entity(entity).try_insert(Focus);
    }
}
