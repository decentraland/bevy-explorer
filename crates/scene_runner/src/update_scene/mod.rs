use bevy::prelude::Plugin;

use self::{
    camera_mode::CameraModePlugin, engine_info::EngineInfoPlugin, pointer_lock::PointerLockPlugin,
    pointer_results::PointerResultPlugin, raycast_result::RaycastResultPlugin,
};

pub mod camera_mode;
pub mod engine_info;
pub mod pointer_lock;
pub mod pointer_results;
pub mod raycast_result;

pub struct SceneInputPlugin;

impl Plugin for SceneInputPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_plugins(EngineInfoPlugin);
        app.add_plugins(RaycastResultPlugin);
        // pointer results need input and a rendered UI, neither of which exists headless,
        // so no result could ever be produced
        if !app
            .world()
            .get_resource::<common::structs::IsHeadless>()
            .is_some_and(|h| h.0)
        {
            app.add_plugins(PointerResultPlugin);
        }
        app.add_plugins(PointerLockPlugin);
        app.add_plugins(CameraModePlugin);
    }
}
