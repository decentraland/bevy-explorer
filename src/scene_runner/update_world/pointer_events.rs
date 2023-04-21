use bevy::prelude::*;

use crate::{
    dcl::interface::ComponentPosition,
    dcl_component::{proto_components::sdk::components::PbPointerEvents, SceneComponentId},
};

use super::AddCrdtInterfaceExt;

pub struct PointerEventsPlugin;

impl Plugin for PointerEventsPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbPointerEvents, PointerEvents>(
            SceneComponentId::POINTER_EVENTS,
            ComponentPosition::EntityOnly,
        );
    }
}

#[derive(Component)]
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