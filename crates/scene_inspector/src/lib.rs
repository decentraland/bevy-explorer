use bevy::prelude::*;
use common::sets::SceneSets;
use dcl_component::ComponentNameRegistry;
use scene_runner::update_scene::raycast_result::SuperUserRaycastScene;

mod active_scene;
mod asset_commands;
mod manual_registry;
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

        // after the scene loop so replies are handled the frame they arrive, and before pruning
        // so a reply that raced a scene breaking is still delivered
        app.add_systems(
            Update,
            (
                snapshot::handle_snapshot_events,
                snapshot::handle_entity_allocated_events,
                snapshot::prune_dead_scene_requests,
            )
                .chain()
                .in_set(SceneSets::PostLoop),
        );
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
