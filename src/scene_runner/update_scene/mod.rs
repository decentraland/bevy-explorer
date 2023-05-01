use bevy::prelude::Plugin;

use self::{pointer_results::PointerResultPlugin, raycast_result::RaycastResultPlugin, engine_info::EngineInfoPlugin};

pub mod pointer_results;
pub mod raycast_result;
pub mod engine_info;

pub struct SceneInputPlugin;

impl Plugin for SceneInputPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_plugin(EngineInfoPlugin);
        app.add_plugin(RaycastResultPlugin);
        app.add_plugin(PointerResultPlugin);
    }
}
