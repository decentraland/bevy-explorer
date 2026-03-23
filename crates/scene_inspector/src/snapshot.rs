use bevy::prelude::*;
use dcl::interface::CrdtStore;
use scene_runner::CrdtSnapshotEvent;
use std::collections::HashMap;

pub type SnapshotCallback = Box<dyn FnOnce(&CrdtStore) + Send + Sync>;

/// Callbacks waiting for a CRDT snapshot from their scene thread.
/// Requests are dispatched immediately via [`SceneResolver::request_snapshot`];
/// this resource stores the callbacks until the snapshot arrives.
#[derive(Resource, Default)]
pub struct PendingSnapshotRequests(pub HashMap<Entity, Vec<SnapshotCallback>>);

impl PendingSnapshotRequests {
    pub fn push(&mut self, entity: Entity, callback: SnapshotCallback) {
        self.0.entry(entity).or_default().push(callback);
    }
}

/// Call any pending callbacks when their snapshot arrives.
pub fn handle_snapshot_events(
    mut events: EventReader<CrdtSnapshotEvent>,
    mut pending: ResMut<PendingSnapshotRequests>,
) {
    for event in events.read() {
        if let Some(callbacks) = pending.0.remove(&event.scene_entity) {
            for cb in callbacks {
                cb(&event.crdt);
            }
        }
    }
}
