use bevy::{
    prelude::*,
    ui::{FocusPolicy, RelativeCursorPosition},
};
use ui_core::ui_actions::{HoverEnter, HoverExit, On};

use crate::{
    update_scene::pointer_results::{UiPointerTarget, UiPointerTargetValue},
    update_world::pointer_events::PointerEvents,
};

use super::UiLink;

pub fn set_ui_pointer_events(
    mut commands: Commands,
    mut pes: Query<
        &mut UiLink,
        (
            With<PointerEvents>,
            Or<(Changed<PointerEvents>, Changed<UiLink>)>,
        ),
    >,
    mut links: Query<&mut UiLink, Without<PointerEvents>>,
    mut removed: RemovedComponents<PointerEvents>,
) {
    for ent in removed.read() {
        let Ok(mut link) = links.get_mut(ent) else {
            continue;
        };

        if let Ok(mut commands) = commands.get_entity(link.ui_entity) {
            commands.remove::<(On<HoverEnter>, On<HoverExit>)>();
        }

        link.bypass_change_detection().interactors.remove("pointer_events");
    }

    for mut link in pes.iter_mut() {
        link.bypass_change_detection().interactors.insert("pointer_events");
    }
}

pub fn manage_scene_ui_interact(
    q: Query<(Entity, &UiLink), Changed<UiLink>>,
    mut commands: Commands,
    mut ui_target: ResMut<UiPointerTarget>,
) {
    for (entity, link) in q.iter() {
        if link.interactors.is_empty() {
            commands
                .entity(link.ui_entity)
                .insert(FocusPolicy::Pass)
                .remove::<(Interaction, On<HoverEnter>, On<HoverExit>)>();
            if ui_target.0.entity() == Some(entity) {
                ui_target.0 = UiPointerTargetValue::None;
            }
        } else if let Ok(mut commands) = commands.get_entity(link.ui_entity) {
            let is_primary = link.is_window_ui;
            commands.try_insert((
                FocusPolicy::Block,
                RelativeCursorPosition::default(),
                Interaction::default(),
                On::<HoverEnter>::new(move |mut ui_target: ResMut<UiPointerTarget>| {
                    debug!("hover enter {entity:?}");
                    if is_primary {
                        ui_target.0 = UiPointerTargetValue::Primary(entity, None);
                    } else {
                        ui_target.0 = UiPointerTargetValue::World(entity, None);
                    }
                }),
                On::<HoverExit>::new(move |mut ui_target: ResMut<UiPointerTarget>| {
                    debug!("hover exit  {entity:?}");
                    if ui_target.0.entity() == Some(entity) {
                        ui_target.0 = UiPointerTargetValue::None;
                    }
                }),
            ));
        }
    }
}
