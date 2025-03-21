use bevy::{prelude::*, ui::FocusPolicy};
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

        if let Some(mut commands) = commands.get_entity(link.ui_entity) {
            commands.remove::<(On<HoverEnter>, On<HoverExit>)>();
        }
    }

    for (ent, link) in pes.iter() {
        if let Some(mut commands) = commands.get_entity(link.ui_entity) {
            let is_primary = link.is_window_ui;
            commands.try_insert((
                FocusPolicy::Block,
                Interaction::default(),
                On::<HoverEnter>::new(move |mut ui_target: ResMut<UiPointerTarget>| {
                    debug!("hover enter {ent:?}");
                    if is_primary {
                        ui_target.0.push(UiPointerTargetValue::Primary(ent));
                    } else {
                        ui_target.0.push(UiPointerTargetValue::World(ent));
                    }
                }),
                On::<HoverExit>::new(move |mut ui_target: ResMut<UiPointerTarget>| {
                    debug!("hover exit  {ent:?}");
                    ui_target.0.retain(|v| {
                        v != &UiPointerTargetValue::Primary(ent)
                            && v != &UiPointerTargetValue::World(ent)
                    });
                }),
            ));
        }
    }
}
