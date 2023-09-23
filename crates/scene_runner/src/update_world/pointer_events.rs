use bevy::prelude::*;
use common::structs::ToolTips;
use input_manager::InputMap;

use crate::update_scene::pointer_results::{PointerTarget, PointerTargetInfo};
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::sdk::components::PbPointerEvents,
    SceneComponentId,
};

use super::AddCrdtInterfaceExt;

pub struct PointerEventsPlugin;

impl Plugin for PointerEventsPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbPointerEvents, PointerEvents>(
            SceneComponentId::POINTER_EVENTS,
            ComponentPosition::EntityOnly,
        );

        app.add_systems(Update, hover_text);
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
pub struct HoverText;

#[allow(clippy::too_many_arguments)]
fn hover_text(
    pointer_events: Query<&PointerEvents>,
    hover_target: Res<PointerTarget>,
    input_map: Res<InputMap>,
    mut tooltip: ResMut<ToolTips>,
) {
    let mut texts = Vec::default();

    if let Some(PointerTargetInfo {
        container,
        distance,
        ..
    }) = hover_target.0
    {
        if let Ok(pes) = pointer_events.get(container) {
            texts = pes
                .msg
                .pointer_events
                .iter()
                .flat_map(|pe| {
                    if let Some(info) = pe.event_info.as_ref() {
                        if info.show_feedback.unwrap_or(true) {
                            if let Some(text) = info.hover_text.as_ref() {
                                return Some((
                                    format!("{} : {}", input_map.get_input(info.button()), text),
                                    info.max_distance.unwrap_or(10.0) > distance.0,
                                ));
                            }
                        }
                    }
                    None
                })
                .collect::<Vec<_>>();
        }
    }

    tooltip.0.insert("pointer_events", texts);
}
