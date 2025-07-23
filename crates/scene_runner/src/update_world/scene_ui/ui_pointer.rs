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
    pes: Query<
        (Entity, &UiLink),
        (
            With<PointerEvents>,
            Or<(Changed<PointerEvents>, Changed<UiLink>)>,
        ),
    >,
    links: Query<&UiLink>,
    mut removed: RemovedComponents<PointerEvents>,
) {
    for ent in removed.read() {
        let Ok(link) = links.get(ent) else {
            continue;
        };

        if let Ok(mut commands) = commands.get_entity(link.ui_entity) {
            commands.remove::<(On<HoverEnter>, On<HoverExit>)>();
        }
    }

    for (ent, link) in pes.iter() {
        if let Ok(mut commands) = commands.get_entity(link.ui_entity) {
            let is_primary = link.is_window_ui;
            commands.try_insert((
                FocusPolicy::Block,
                RelativeCursorPosition::default(),
                Interaction::default(),
                On::<HoverEnter>::new(move |mut ui_target: ResMut<UiPointerTarget>| {
                    debug!("hover enter {ent:?}");
                    if is_primary {
                        ui_target.0 = UiPointerTargetValue::Primary(ent, None);
                    } else {
                        ui_target.0 = UiPointerTargetValue::World(ent, None);
                    }
                }),
                On::<HoverExit>::new(move |mut ui_target: ResMut<UiPointerTarget>| {
                    debug!("hover exit  {ent:?}");
                    if ui_target.0.entity() == Some(ent) {
                        ui_target.0 = UiPointerTargetValue::None;
                    }
                }),
            ));
        }
    }
}
