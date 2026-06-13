use bevy::prelude::*;
use dcl_component::ComponentNameRegistry;
use scene_runner::update_scene::raycast_result::SuperUserRaycastScene;

mod active_scene;
mod asset_commands;
mod manual_registry;
mod message_bus;
mod read_commands;
pub mod snapshot;
mod write_commands;

pub use active_scene::ActiveInspectionScene;
pub use snapshot::PendingSnapshotRequests;

pub struct SceneInspectorPlugin;

impl Plugin for SceneInspectorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ComponentNameRegistry>();
        app.init_resource::<ActiveInspectionScene>();
        app.init_resource::<PendingSnapshotRequests>();
        app.init_resource::<snapshot::PendingEntityAllocations>();

        manual_registry::register_engine_components(app);

        read_commands::add_read_commands(app);
        write_commands::add_write_commands(app);
        asset_commands::add_asset_commands(app);
        // editor page<->scene message bus (/editor_send, /editor_poll) — the
        // transport the host UI uses; not provided upstream.
        message_bus::add_message_bus_commands(app);

        app.add_systems(Update, snapshot::handle_snapshot_events);
        app.add_systems(Update, snapshot::handle_entity_allocated_events);
        app.add_systems(Update, sync_super_user_raycast_target);
    }
}

// Mirror the *explicitly pinned* inspection scene (/set_scene <hash>) into the raycast target
// resource. Only set when the super-user scene has deliberately chosen a target — None (follow
// player / unpinned) leaves super-user raycasts at their normal behaviour.
fn sync_super_user_raycast_target(
    active: Res<ActiveInspectionScene>,
    mut target: ResMut<SuperUserRaycastScene>,
) {
    if target.0 != active.0 {
        target.0 = active.0;
    }
}
