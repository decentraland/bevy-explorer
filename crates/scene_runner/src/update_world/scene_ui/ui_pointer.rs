use bevy::{
    diagnostic::FrameCount,
    prelude::*,
    ui::{FocusPolicy, RelativeCursorPosition},
};
use ui_core::ui_actions::{HoverEnter, HoverExit, On};

use crate::{
    update_scene::pointer_results::{UiPointerTarget, UiPointerTargetValue},
    update_world::pointer_events::PointerEvents,
};

use super::UiLink;

#[derive(Component)]
pub struct UiPointerChanged;

/// Frame on which a UI node was last hovered. Set on `HoverEnter`, removed on
/// `HoverExit`. On exit we fall back to the highest-stamped node that is still
/// hovered (rather than clearing the pointer target to `None`): a node held down
/// stays `Interaction::Pressed`, so it never re-emits `HoverEnter` and can't
/// otherwise reclaim the target after the cursor crosses another node.
#[derive(Component)]
pub struct UiHoverTime(pub u32);

pub fn set_ui_pointer_events(
    mut commands: Commands,
    mut pes: Query<
        (Entity, &mut UiLink),
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
        if let Ok(mut commands) = commands.get_entity(ent) {
            commands.try_insert(UiPointerChanged);
        }

        link.bypass_change_detection()
            .interactors
            .remove("pointer_events");
    }

    for (ent, mut link) in pes.iter_mut() {
        if let Ok(mut commands) = commands.get_entity(ent) {
            commands.try_insert(UiPointerChanged);
        }

        link.bypass_change_detection()
            .interactors
            .insert("pointer_events");
    }
}

pub fn manage_scene_ui_interact(
    q: Query<(Entity, &UiLink), Or<(Changed<UiLink>, Added<UiPointerChanged>)>>,
    mut commands: Commands,
    mut ui_target: ResMut<UiPointerTarget>,
) {
    for (entity, link) in q.iter() {
        if link.interactors.is_empty() {
            commands
                .entity(link.ui_entity)
                .try_insert(FocusPolicy::Pass)
                .remove::<(Interaction, On<HoverEnter>, On<HoverExit>)>();
            // The node can no longer emit HoverExit to clear its own stamp.
            if let Ok(mut ec) = commands.get_entity(entity) {
                ec.remove::<UiHoverTime>();
            }
            if ui_target.0.entity() == Some(entity) {
                ui_target.0 = UiPointerTargetValue::None;
            }
        } else if let Ok(mut commands) = commands.get_entity(link.ui_entity) {
            let is_primary = link.is_window_ui;
            commands.try_insert((
                FocusPolicy::Block,
                RelativeCursorPosition::default(),
                Interaction::default(),
                On::<HoverEnter>::new(
                    move |mut ui_target: ResMut<UiPointerTarget>,
                          frame: Res<FrameCount>,
                          mut commands: Commands| {
                        debug!("hover enter {entity:?}");
                        if let Ok(mut ec) = commands.get_entity(entity) {
                            ec.try_insert(UiHoverTime(frame.0));
                        }
                        if is_primary {
                            ui_target.0 = UiPointerTargetValue::Primary(entity, None);
                        } else {
                            ui_target.0 = UiPointerTargetValue::World(entity, None);
                        }
                    },
                ),
                On::<HoverExit>::new(
                    move |mut ui_target: ResMut<UiPointerTarget>,
                          mut commands: Commands,
                          hovered: Query<(Entity, &UiHoverTime, &UiLink)>| {
                        debug!("hover exit  {entity:?}");
                        if let Ok(mut ec) = commands.get_entity(entity) {
                            ec.remove::<UiHoverTime>();
                        }
                        // Only the current target's exit needs repair.
                        if ui_target.0.entity() != Some(entity) {
                            return;
                        }
                        // Removal above is deferred, so skip the exiting node and
                        // fall back to the most-recently-entered node still hovered.
                        let fallback = hovered
                            .iter()
                            .filter(|(e, _, _)| *e != entity)
                            .max_by_key(|(_, t, _)| t.0);
                        ui_target.0 = match fallback {
                            Some((e, _, link)) if link.is_window_ui => {
                                UiPointerTargetValue::Primary(e, None)
                            }
                            Some((e, _, _)) => UiPointerTargetValue::World(e, None),
                            None => UiPointerTargetValue::None,
                        };
                    },
                ),
            ));
        }

        commands.entity(entity).remove::<UiPointerChanged>();
    }
}
