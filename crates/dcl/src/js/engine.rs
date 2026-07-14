use std::{cell::RefCell, rc::Rc, sync::Arc};

// Engine module
use bevy::log::{debug, info, warn};
#[cfg(feature = "span_scene_loop")]
use bevy::log::{info_span, tracing::span::EnteredSpan};
use common::structs::{CameraFov, GlobalCrdtStateUpdate, TimeOfDay};
use dcl_component::{DclReader, Localizer, SceneCrdtTimestamp, SceneOrigin};
use tokio::sync::{broadcast::error::TryRecvError, Mutex};

use crate::{
    crdt::{append_component, delete_entity, put_component},
    interface::{crdt_context::CrdtContext, CrdtType},
    js::{
        AllocatorContext, CommunicatedWithRenderer, FilteredCrdtStore, RendererStore,
        SceneResponseSender, ShuttingDown,
    },
    AllocError, CrdtComponentInterfaces, CrdtStore, RendererResponse, RpcCalls, SceneElapsedTime,
    SceneLogMessage, SceneResponse,
};

use super::State;

/// Returns whether this scene runs in authoritative-server role. Read synchronously
/// from the scene's CrdtContext (seeded from the `IsServer` engine resource). MUST stay
/// synchronous — an async op would return a Promise to JS, and `!!Promise` is always
/// true, making every client believe it is the server.
pub fn op_is_server(state: Rc<RefCell<impl State>>) -> bool {
    state.borrow().borrow::<CrdtContext>().is_server
}

/// Localize the payload within a CRDT wire-format message.
/// CRDT PutComponent format: length(4) + type(4) + entity(4) + component(4) + timestamp(4) + content_len(4) + payload
fn localize_crdt_message(
    data: Vec<u8>,
    localizer: &Localizer,
    scene_origin: &SceneOrigin,
) -> Vec<u8> {
    // Minimum size for a PutComponent message with payload: 24 bytes header + payload
    if data.len() < 24 {
        return data;
    }

    let content_len = u32::from_le_bytes([data[20], data[21], data[22], data[23]]) as usize;
    let payload_start = 24;
    let payload_end = payload_start + content_len;

    if payload_end > data.len() || content_len == 0 {
        return data;
    }

    let new_payload = localizer.localize_payload(&data[payload_start..payload_end], scene_origin);

    // Rebuild the message with the new payload
    let entity_id: dcl_component::SceneEntityId = {
        let mut r = DclReader::new(&data[8..12]);
        r.read().unwrap()
    };
    let component_id: dcl_component::SceneComponentId = {
        let mut r = DclReader::new(&data[12..16]);
        r.read().unwrap()
    };
    let timestamp: dcl_component::SceneCrdtTimestamp = {
        let mut r = DclReader::new(&data[16..20]);
        r.read().unwrap()
    };

    put_component(&entity_id, &component_id, &timestamp, Some(&new_payload))
}

pub fn crdt_send_to_renderer(op_state: Rc<RefCell<impl State>>, messages: &[u8]) {
    let mut op_state = op_state.borrow_mut();
    let elapsed_time = op_state.borrow::<SceneElapsedTime>().0;
    let logs = op_state.take::<Vec<SceneLogMessage>>();
    op_state.put(Vec::<SceneLogMessage>::default());
    let mut entity_map = op_state.take::<CrdtContext>();
    let mut crdt_store = op_state.take::<CrdtStore>();
    let mut filtered_store = op_state.take::<FilteredCrdtStore>();
    let mut allocator = op_state.take::<AllocatorContext>();
    let writers = op_state.take::<CrdtComponentInterfaces>();
    let mut stream = DclReader::new(messages);
    debug!("op_crdt_send_to_renderer BATCH len: {}", stream.len());

    // collect commands; unrecognized components are captured in the sidecar for the inspector, and
    // every entity (recognized + filtered) is tracked in the allocator context for entity allocation
    crdt_store.process_message_stream(
        &mut entity_map,
        &writers,
        &mut stream,
        true,
        Some(&mut filtered_store.0),
        Some(&mut allocator.0),
    );
    // flush the allocator's nascent births into its live table so new_in_range sees them (we don't
    // use the census itself — the renderer is driven by entity_map's census above).
    let _ = allocator.0.take_census();

    let census = entity_map.take_census();
    crdt_store.clean_up(&census.died);
    let updates = crdt_store.take_updates();

    let rpc_calls = std::mem::take(op_state.borrow_mut::<RpcCalls>());

    let sender = op_state.borrow_mut::<SceneResponseSender>();
    sender
        .try_send(SceneResponse::Ok(
            entity_map.scene_id,
            census,
            updates,
            SceneElapsedTime(elapsed_time),
            logs,
            rpc_calls,
        ))
        .expect("failed to send to renderer");

    op_state.put(writers);
    op_state.put(entity_map);
    op_state.put(crdt_store);
    op_state.put(filtered_store);
    op_state.put(allocator);
}

pub async fn op_crdt_recv_from_renderer(op_state: Rc<RefCell<impl State>>) -> Vec<Vec<u8>> {
    #[cfg(feature = "span_scene_loop")]
    {
        let span = op_state.borrow_mut().try_take::<EnteredSpan>();
        drop(span); // don't hold it over the await point so we get a clearer view of when js is running
    }

    debug!("op_crdt_recv_from_renderer");

    // Receive messages in a loop, handling snapshot/allocation requests immediately (no RefMut held
    // across the await point) and looping for the next.  Exits with the first Ok/shutdown response —
    // these are what actually tick the scene. `injected` accumulates entity-instantiation
    // put_components from AllocateEntity requests so they ride along with that next tick.
    let mut injected: Vec<Vec<u8>> = Vec::new();
    let response = loop {
        let receiver = op_state
            .borrow_mut()
            .borrow_mut::<Arc<Mutex<tokio::sync::mpsc::UnboundedReceiver<RendererResponse>>>>()
            .clone();
        let response = receiver.lock().await.recv().await;

        match response {
            Some(RendererResponse::GetCrdtSnapshot) => {
                let crdt_store = op_state.borrow_mut().take::<CrdtStore>();
                let mut snapshot = crdt_store.clone();
                op_state.borrow_mut().put(crdt_store);
                // Merge renderer→scene components so the snapshot includes engine-managed
                // values (EngineInfo, RaycastResult, etc.) alongside scene-set components.
                let renderer_store = op_state.borrow_mut().take::<RendererStore>();
                snapshot.merge_newer(renderer_store.0.clone());
                op_state.borrow_mut().put(renderer_store);
                // Merge the sidecar so the snapshot also carries custom (filtered-out) components
                // as raw bytes; these never reach the renderer, only the inspector.
                let filtered_store = op_state.borrow_mut().take::<FilteredCrdtStore>();
                snapshot.merge_newer(filtered_store.0.clone());
                op_state.borrow_mut().put(filtered_store);
                let scene_id = op_state.borrow_mut().borrow::<CrdtContext>().scene_id;
                op_state
                    .borrow_mut()
                    .borrow_mut::<SceneResponseSender>()
                    .try_send(SceneResponse::CrdtSnapshot(scene_id, snapshot))
                    .expect("failed to send crdt snapshot");
                continue;
            }
            Some(RendererResponse::AllocateEntity {
                component_id,
                data,
                count,
                explicit_ids,
            }) => {
                // Allocate ids from the authoritative allocator (collision-free, correctly
                // generationed) and reply immediately. Buffer the instantiating put_components so
                // they're delivered with the next Ok tick rather than ticking the scene here — the
                // scene's @dcl/ecs then adopts the entities on receive, before its update() runs.
                //
                // With `explicit_ids`, instantiate those exact ids instead of allocating fresh —
                // used to recreate entities at their original ids on a freshly-reloaded scene. A
                // requested id that's already alive (a collision) yields an `Err` in its slot, so
                // the caller can surface it. Without it, `count` fresh ids.
                //
                // IMPORTANT: `component_id` MUST be a non-engine-recognized (custom) component.
                // Engine-recognized components flow renderer→scene one-way (the scene never echoes
                // them back), so an instantiation written with one would never reach the renderer's
                // store and the value would be lost — only a custom component round-trips.
                //
                // NOTE: `count > 1` instantiates every entity with the same component; only single
                // allocation is used today (batched per-entity instantiation is a later fix).
                let mut allocator = op_state.borrow_mut().take::<AllocatorContext>();
                let mut filtered_store = op_state.borrow_mut().take::<FilteredCrdtStore>();
                let scene_id = allocator.0.scene_id;
                // One result per requested slot, in order: a caller-specified id (validated below)
                // or a freshly-allocated one, else the reason it couldn't be allocated. authored
                // entities live above the reserved-static range (512); avoid the u16::MAX wrap
                // sentinel used by new_in_range's `last_new`.
                let results: Vec<Result<dcl_component::SceneEntityId, AllocError>> =
                    match &explicit_ids {
                        Some(protos) => protos
                            .iter()
                            .map(|p| {
                                let entity = dcl_component::SceneEntityId::from_proto_u32(*p);
                                if allocator.0.alloc_explicit(entity) {
                                    Ok(entity)
                                } else {
                                    warn!("AllocateEntity: id {entity:?} already live (collision)");
                                    Err(AllocError::Collision(entity))
                                }
                            })
                            .collect(),
                        None => (0..count)
                            .map(|_| {
                                allocator
                                    .0
                                    .new_in_range(&(512..=u16::MAX - 1))
                                    .ok_or_else(|| {
                                        warn!("AllocateEntity: no free entity id");
                                        AllocError::NoFreeId
                                    })
                            })
                            .collect(),
                    };
                for id in results.iter().filter_map(|r| r.as_ref().ok()).copied() {
                    // Also record the instantiation in the sidecar so /crdt_snapshot reflects it —
                    // the scene never echoes the injected (renderer→scene) component back, so without
                    // this the editor's view of the component would vanish on the next reload.
                    filtered_store.0.try_update(
                        component_id,
                        CrdtType::LWW_ANY,
                        id,
                        SceneCrdtTimestamp(1),
                        Some(&mut DclReader::new(&data)),
                    );
                    injected.push(put_component(
                        &id,
                        &component_id,
                        &SceneCrdtTimestamp(1),
                        Some(&data),
                    ));
                }
                op_state.borrow_mut().put(allocator);
                op_state.borrow_mut().put(filtered_store);
                debug!("AllocateEntity: {results:?}");
                let _ = op_state
                    .borrow_mut()
                    .borrow_mut::<SceneResponseSender>()
                    .try_send(SceneResponse::EntityAllocated(scene_id, results));
                continue;
            }
            other => break other,
        }
    };

    let mut op_state = op_state.borrow_mut();
    #[cfg(feature = "span_scene_loop")]
    {
        let span = info_span!("js update").entered();
        op_state.put(span);
    }

    let mut entity_map = op_state.take::<CrdtContext>();
    let mut renderer_state = op_state.take::<RendererStore>();
    let writers = op_state.take::<CrdtComponentInterfaces>();

    let mut results = match response {
        Some(RendererResponse::Ok(updates, census)) => {
            // Lead with any buffered entity instantiations so the scene adopts the new entities
            // before applying this tick's component updates.
            let mut results = std::mem::take(&mut injected);
            // TODO: consider writing directly into a v8 buffer
            for (component_id, lww) in updates.lww.iter() {
                for (entity_id, data) in lww.last_write.iter() {
                    results.push(put_component(
                        entity_id,
                        component_id,
                        &data.timestamp,
                        data.is_some.then_some(data.data.as_slice()),
                    ));
                }
            }
            for (component_id, go) in updates.go.iter() {
                for (entity_id, data) in go.0.iter() {
                    for item in data.iter() {
                        results.push(append_component(entity_id, component_id, &item.data));
                    }
                }
            }

            // store the updates + apply the census's deletions to the mirror
            renderer_state.0.update_from(updates, &census);

            // Engine-initiated deletes: mark them dead in the entity map and the allocator (the
            // scene won't re-send these as DeleteEntity stream messages, so process_message's
            // alloc.kill never sees them), and forward a DeleteEntity to the SDK so the scene
            // deletes them too. (update_from already dropped them from the RendererStore.)
            let mut allocator = op_state.take::<AllocatorContext>();
            for entity_id in census.died.iter() {
                entity_map.kill(*entity_id);
                allocator.0.kill(*entity_id);
                results.push(delete_entity(entity_id));
            }
            op_state.put(allocator);
            // census.born is reserved for engine-created entities (none yet).

            results
        }
        // AllocateEntity is handled in the receive loop above (it doesn't tick the scene).
        Some(RendererResponse::AllocateEntity { .. }) => unreachable!(),
        None => {
            // channel has been closed, shutdown gracefully
            info!(
                "{}: shutting down",
                std::thread::current().name().unwrap_or("(webworker)")
            );
            op_state.put(ShuttingDown);
            Default::default()
        }
        // GetCrdtSnapshot is handled before this match in the loop above.
        Some(RendererResponse::GetCrdtSnapshot) => unreachable!(),
    };

    let mut global_update_receiver =
        op_state.take::<tokio::sync::broadcast::Receiver<GlobalCrdtStateUpdate>>();
    loop {
        match global_update_receiver.try_recv() {
            Ok(next) => match next {
                GlobalCrdtStateUpdate::Crdt(data, localizer) => {
                    let data = match localizer {
                        Localizer::None => data,
                        Localizer::Unimplemented => {
                            warn!("received global CRDT update with Unimplemented localizer");
                            data
                        }
                        _ => {
                            let scene_origin = op_state.borrow::<SceneOrigin>();
                            localize_crdt_message(data, &localizer, scene_origin)
                        }
                    };
                    let mut stream = DclReader::new(&data);
                    renderer_state.0.process_message_stream(
                        &mut entity_map,
                        &writers,
                        &mut stream,
                        false,
                        None,
                        None,
                    );
                    results.push(data);
                }
                GlobalCrdtStateUpdate::Time(time) => {
                    let time_of_day = op_state.borrow_mut::<TimeOfDay>();
                    time_of_day.time = time;
                }
                GlobalCrdtStateUpdate::CameraFov(fov_y) => {
                    let camera_fov = op_state.borrow_mut::<CameraFov>();
                    camera_fov.0 = fov_y;
                }
            },
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Lagged(_)) => (), // continue on with whatever we can still get
            Err(TryRecvError::Closed) => {
                warn!("global receiver shut down");
                break;
            }
        }
    }

    op_state.put(renderer_state);
    op_state.put(entity_map);
    op_state.put(writers);
    op_state.put(global_update_receiver);
    op_state.put(CommunicatedWithRenderer);

    results
}
