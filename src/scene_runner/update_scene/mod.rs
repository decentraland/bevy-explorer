use bevy::prelude::Plugin;

use self::raycast_result::RaycastResultPlugin;

pub mod raycast_result;

pub struct SceneInputPlugin;

impl Plugin for SceneInputPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_plugin(RaycastResultPlugin);
    }
}
