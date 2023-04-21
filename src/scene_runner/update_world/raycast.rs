// TODO
// [x] - handle continuous properly
// [x] - don't run every renderer frame
// [/] - then prevent scene execution until raycasts are run (not exactly required now, we run raycasts once on first frame after request arrives)
// - probably change renderer context to contain frame number as well as dt so we can track precisely track run state
// - move into scene loop
// - consider how global raycasts interact with this setup

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
    pub last_run: f32,
}

impl From<PbRaycast> for Raycast {
    fn from(value: PbRaycast) -> Self {
        Self {
            raycast: value,
            last_run: 0.0,
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
