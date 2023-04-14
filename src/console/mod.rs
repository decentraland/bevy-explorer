use bevy::prelude::*;

pub struct ConsolePlugin;

impl Plugin for ConsolePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(bevy_console::ConsolePlugin);
    }
}