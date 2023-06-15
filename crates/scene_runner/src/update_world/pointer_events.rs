use bevy::prelude::*;
use bevy_mod_billboard::{BillboardDepth, BillboardTextBundle};

use crate::update_scene::pointer_results::PointerTarget;
use dcl::interface::ComponentPosition;
use dcl_component::{proto_components::sdk::components::PbPointerEvents, SceneComponentId};
use ui_core::TITLE_TEXT_STYLE;

use super::AddCrdtInterfaceExt;

pub struct PointerEventsPlugin;

impl Plugin for PointerEventsPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbPointerEvents, PointerEvents>(
            SceneComponentId::POINTER_EVENTS,
            ComponentPosition::EntityOnly,
        );

        app.add_system(hover_text);
    }
}

#[derive(Component, Debug)]
pub struct PointerEvents {
    pub msg: PbPointerEvents,
}

impl From<PbPointerEvents> for PointerEvents {
    fn from(pb_pointer_events: PbPointerEvents) -> Self {
        Self {
            msg: pb_pointer_events,
        }
    }
}

#[derive(Component)]
pub struct HoverText(Entity);
fn hover_text(
    mut commands: Commands,
    texts: Query<(
        &PointerEvents,
        &GlobalTransform,
        Changed<PointerEvents>,
        Changed<GlobalTransform>,
    )>,
    cur_text: Query<Entity, With<HoverText>>,
    hover_target: Res<PointerTarget>,
) {
    if let PointerTarget::Some { container, .. } = *hover_target {
        if let Ok((pes, gt, changed_1, changed_2)) = texts.get(container) {
            // check existing
            if cur_text.get(container).is_ok() && !changed_1 && !changed_2 {
                return;
            }

            // add new
            for pe in pes.msg.pointer_events.iter() {
                if let Some(info) = pe.event_info.as_ref() {
                    if info.show_feedback.unwrap_or(true) {
                        if let Some(text) = info.hover_text.as_ref() {
                            commands.entity(container).with_children(|c| {
                                let scale = gt.to_scale_rotation_translation().0;

                                c.spawn((
                                    BillboardTextBundle {
                                        text: Text::from_section(
                                            text.clone(),
                                            TextStyle {
                                                font_size: 50.0,
                                                color: Color::WHITE,
                                                ..TITLE_TEXT_STYLE.get().unwrap().clone()
                                            },
                                        ),
                                        billboard_depth: BillboardDepth(false),
                                        transform: Transform::from_scale(
                                            Vec3::splat(0.01) * 1.0 / scale,
                                        ),
                                        ..Default::default()
                                    },
                                    HoverText(container),
                                ));
                            });
                        }
                    }
                }
            }
        }
    }

    // remove any existing
    for existing in cur_text.iter() {
        commands.entity(existing).despawn_recursive();
    }
}
