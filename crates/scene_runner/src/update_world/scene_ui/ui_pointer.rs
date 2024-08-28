use bevy::{prelude::*, ui::FocusPolicy};
use ui_core::ui_actions::{HoverEnter, HoverExit, On};

use crate::{
    update_scene::pointer_results::UiPointerTarget, update_world::pointer_events::PointerEvents,
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
) {
    for (ent, link) in pes.iter() {
        if let Some(mut commands) = commands.get_entity(link.ui_entity) {
            let is_primary = link.is_window_ui;
            commands.try_insert((
                FocusPolicy::Block,
                Interaction::default(),
                On::<HoverEnter>::new(move |mut ui_target: ResMut<UiPointerTarget>| {
                    if is_primary {
                        *ui_target = UiPointerTarget::Primary(ent);
                    } else {
                        *ui_target = UiPointerTarget::World(ent);
                    }
                }),
                On::<HoverExit>::new(move |mut ui_target: ResMut<UiPointerTarget>| {
                    if *ui_target == UiPointerTarget::Primary(ent)
                        || *ui_target == UiPointerTarget::World(ent)
                    {
                        *ui_target = UiPointerTarget::None;
                    };
                }),
            ));
        }
    }
}
