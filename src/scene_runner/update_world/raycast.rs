use bevy::prelude::*;

use crate::{
    dcl::interface::ComponentPosition,
    dcl_component::{proto_components::sdk::components::PbRaycast, SceneComponentId},
    scene_runner::SceneSets,
};

use super::AddCrdtInterfaceExt;

pub struct RaycastPlugin;

#[derive(Component, Debug)]
pub struct Raycast(PbRaycast);

impl From<PbRaycast> for Raycast {
    fn from(value: PbRaycast) -> Self {
        Self(value)
    }
}

impl Plugin for RaycastPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbRaycast, Raycast>(
            SceneComponentId::RAYCAST,
            ComponentPosition::EntityOnly,
        );
        app.add_system(print_raycasts.in_set(SceneSets::PostLoop));
    }
}

fn print_raycasts(q: Query<(Entity, &Raycast)>) {
    for (e, ray) in q.iter() {
        debug!("{e:?} has raycast request: {ray:?}");
    }
}
