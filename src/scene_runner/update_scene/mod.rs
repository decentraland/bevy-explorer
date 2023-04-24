use bevy::prelude::Plugin;

use self::{pointer_results::PointerResultPlugin, raycast_result::RaycastResultPlugin};

pub mod pointer_results;
pub mod raycast_result;

pub struct SceneInputPlugin;

impl Plugin for SceneInputPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_plugin(RaycastResultPlugin);
        app.add_plugin(PointerResultPlugin);
    }
}
