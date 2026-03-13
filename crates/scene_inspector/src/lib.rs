use bevy::prelude::*;
use dcl_component::ComponentNameRegistry;

mod active_scene;
mod manual_registry;
mod read_commands;
pub mod snapshot;

pub use active_scene::ActiveInspectionScene;
pub use snapshot::PendingSnapshotRequests;

pub struct SceneInspectorPlugin;

impl Plugin for SceneInspectorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ComponentNameRegistry>();
        app.init_resource::<ActiveInspectionScene>();
        app.init_resource::<PendingSnapshotRequests>();

        manual_registry::register_engine_components(app);

        read_commands::add_read_commands(app);

        app.add_systems(Update, snapshot::handle_snapshot_events);
    }
}
