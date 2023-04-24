use bevy::prelude::*;

use crate::{
    dcl::interface::ComponentPosition,
    dcl_component::{proto_components::sdk::components::PbRaycast, SceneComponentId},
};

use super::AddCrdtInterfaceExt;

pub struct RaycastPlugin;

#[derive(Component, Debug)]
pub struct Raycast {
    pub raycast: PbRaycast,
    pub last_run: u32,
}

impl From<PbRaycast> for Raycast {
    fn from(value: PbRaycast) -> Self {
        Self {
            raycast: value,
            last_run: 0,
        }
    }
}

impl Plugin for RaycastPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbRaycast, Raycast>(
            SceneComponentId::RAYCAST,
            ComponentPosition::EntityOnly,
        );
    }
}
