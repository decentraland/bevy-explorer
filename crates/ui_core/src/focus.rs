use bevy::prelude::*;
use dcl_component::proto_components::sdk::components::common::InputAction;
use input_manager::{Action, InputManager, InputPriority, InputType, SystemAction};

use crate::ui_actions::{UiActionPriority, UiFocusActionSet};

use super::ui_actions::UiActionSet;
use common::sets::SceneSets;

#[derive(Component)]
pub struct Focus;

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
    focus_elements: Query<(Entity, Ref<Focus>)>,
    mut input_manager: InputManager,
) {
    let refocussed = input_manager.just_down(InputAction::IaPointer, InputPriority::Focus)
        || input_manager.just_down(SystemAction::Cancel, InputPriority::CancelFocus)
        || focus_elements.iter().any(|(_, focus)| focus.is_changed());

    if refocussed {
        let mut any_still_focussed = false;
        for (entity, ref_focus) in focus_elements.iter() {
            if !ref_focus.is_changed() {
                commands.entity(entity).remove::<Focus>();
                debug!("defocus {:?}", entity);
            } else {
                any_still_focussed = true;
            }
        }

        if any_still_focussed {
            debug!("still focus");
            input_manager.priorities().reserve(
                InputType::Action(Action::System(SystemAction::Cancel)),
                InputPriority::CancelFocus,
            );
            input_manager.priorities().reserve(
                InputType::Action(Action::Scene(InputAction::IaAny)),
                InputPriority::Focus,
            );
        } else {
            debug!("not focus");
            input_manager.priorities().release(
                InputType::Action(Action::System(SystemAction::Cancel)),
                InputPriority::CancelFocus,
            );
            input_manager.priorities().release(
                InputType::Action(Action::Scene(InputAction::IaAny)),
                InputPriority::Focus,
            );
        }
    }
}

#[allow(clippy::type_complexity)]
fn focus(
    mut commands: Commands,
    focused_elements: Query<(Entity, &Interaction, Option<&UiActionPriority>), With<Focusable>>,
    input_manager: InputManager,
) {
    for (entity, interaction, maybe_priority) in focused_elements.iter() {
        if interaction != &Interaction::None
            && input_manager.just_down(
                InputAction::IaPointer,
                maybe_priority.copied().unwrap_or_default().0,
            )
        {
            commands.entity(entity).try_insert(Focus);
            debug!("focus {:?}", entity);
        }
    }
}
