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
        app.add_plugins(PointerResultPlugin);
        app.add_plugins(PointerLockPlugin);
        app.add_plugins(CameraModePlugin);
    }
}
