use bevy::prelude::*;
use dcl::{interface::CrdtStore, AllocError};
use dcl_component::SceneEntityId;
use scene_runner::{
    renderer_context::RendererSceneContext, CrdtSnapshotEvent, EntityAllocatedEvent,
};
use std::collections::HashMap;

pub type SnapshotCallback = Box<dyn FnOnce(&CrdtStore) + Send + Sync>;
pub type AllocCallback = Box<dyn FnOnce(&[Result<SceneEntityId, AllocError>]) + Send + Sync>;

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

/// Callbacks waiting for an entity-allocation response (mirrors [`PendingSnapshotRequests`]).
#[derive(Resource, Default)]
pub struct PendingEntityAllocations(pub HashMap<Entity, Vec<AllocCallback>>);

impl PendingEntityAllocations {
    pub fn push(&mut self, entity: Entity, callback: AllocCallback) {
        self.0.entry(entity).or_default().push(callback);
    }
}

/// Call any pending allocation callbacks when their ids arrive, and add the new ids to the scene's
/// `nascent` set so the renderer spawns their bevy entities immediately — directly, not via a
/// `census.born` tick (which a paused scene never produces) — so the editor's subsequent component
/// writes land on an entity that already exists. `nascent` is extended (not assigned) in
/// receive_scene_updates, so a later scene-reported birth can't clobber these.
pub fn handle_entity_allocated_events(
    mut events: EventReader<EntityAllocatedEvent>,
    mut pending: ResMut<PendingEntityAllocations>,
    mut scenes: Query<&mut RendererSceneContext>,
) {
    for event in events.read() {
        if let Ok(mut ctx) = scenes.get_mut(event.scene_entity) {
            ctx.nascent.extend(
                event
                    .results
                    .iter()
                    .filter_map(|r| r.as_ref().ok())
                    .copied(),
            );
        }
        // One EntityAllocated event corresponds to one AllocateEntity request, so fire exactly one
        // callback (FIFO) — not all queued callbacks. The worker processes requests in order and the
        // response channel is ordered, so request order == event order. Firing every callback would
        // deliver this event's ids to all pending requests, cross-wiring concurrent allocations: a
        // later request gets an earlier one's id (a duplicate), and its own event then finds no
        // callback and is dropped.
        let drained = if let Some(callbacks) = pending.0.get_mut(&event.scene_entity) {
            if !callbacks.is_empty() {
                let cb = callbacks.remove(0);
                cb(&event.results);
            }
            callbacks.is_empty()
        } else {
            false
        };
        if drained {
            pending.0.remove(&event.scene_entity);
        }
    }
}

/// Drop pending callbacks for scenes that can no longer reply: the scene entity is gone, its worker
/// is no longer live (a broken scene has dropped its thread handle and never comes back), or the
/// worker has exited and closed its end of the renderer channel.
/// Dropping a callback drops the oneshot sender it captured, so the console reports the command
/// as cancelled instead of polling it forever. Runs after the reply handlers so a reply received
/// in the same frame the scene broke is still delivered.
pub fn prune_dead_scene_requests(
    mut snapshots: ResMut<PendingSnapshotRequests>,
    mut allocations: ResMut<PendingEntityAllocations>,
    scenes: Query<&RendererSceneContext>,
) {
    let alive = |ent: &Entity| {
        scenes
            .get(*ent)
            .is_ok_and(|ctx| ctx.sender().is_some_and(|sender| !sender.is_closed()))
    };
    snapshots.0.retain(|ent, _| alive(ent));
    allocations.0.retain(|ent, _| alive(ent));
}
