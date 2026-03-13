use std::{cell::RefCell, rc::Rc, sync::Arc};

// Engine module
use bevy::log::{debug, info, warn};
#[cfg(feature = "span_scene_loop")]
use bevy::log::{info_span, tracing::span::EnteredSpan};
use common::structs::{GlobalCrdtStateUpdate, TimeOfDay};
use dcl_component::{DclReader, Localizer, SceneOrigin};
use tokio::sync::{broadcast::error::TryRecvError, Mutex};

use crate::{
    crdt::{append_component, put_component},
    interface::crdt_context::CrdtContext,
    js::{CommunicatedWithRenderer, RendererStore, SceneResponseSender, ShuttingDown},
    CrdtComponentInterfaces, CrdtStore, RendererResponse, RpcCalls, SceneElapsedTime,
    SceneLogMessage, SceneResponse,
};

use super::State;

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
    let writers = op_state.take::<CrdtComponentInterfaces>();
    let mut stream = DclReader::new(messages);
    debug!("op_crdt_send_to_renderer BATCH len: {}", stream.len());

    // collect commands
    crdt_store.process_message_stream(&mut entity_map, &writers, &mut stream, true);

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
}

pub async fn op_crdt_recv_from_renderer(op_state: Rc<RefCell<impl State>>) -> Vec<Vec<u8>> {
    #[cfg(feature = "span_scene_loop")]
    {
        let span = op_state.borrow_mut().try_take::<EnteredSpan>();
        drop(span); // don't hold it over the await point so we get a clearer view of when js is running
    }

    debug!("op_crdt_recv_from_renderer");

    // Receive messages in a loop, handling any snapshot requests immediately (no RefMut held
    // across the await point).  Exits with the first non-snapshot response.
    let (response, receiver) = loop {
        let receiver = op_state
            .borrow_mut()
            .borrow_mut::<Arc<Mutex<tokio::sync::mpsc::Receiver<RendererResponse>>>>()
            .clone();
        let response = receiver.lock().await.recv().await;

        if let Some(RendererResponse::GetCrdtSnapshot) = &response {
            let crdt_store = op_state.borrow_mut().take::<CrdtStore>();
            let mut snapshot = crdt_store.clone();
            op_state.borrow_mut().put(crdt_store);
            // Merge renderer→scene components so the snapshot includes engine-managed
            // values (EngineInfo, RaycastResult, etc.) alongside scene-set components.
            let renderer_store = op_state.borrow_mut().take::<RendererStore>();
            snapshot.update_from(renderer_store.0.clone());
            op_state.borrow_mut().put(renderer_store);
            let scene_id = op_state.borrow_mut().borrow::<CrdtContext>().scene_id;
            op_state
                .borrow_mut()
                .borrow_mut::<SceneResponseSender>()
                .try_send(SceneResponse::CrdtSnapshot(scene_id, snapshot))
                .expect("failed to send crdt snapshot");
            continue;
        }
        break (response, receiver);
    };

    let mut op_state = op_state.borrow_mut();
    #[cfg(feature = "span_scene_loop")]
    {
        let span = info_span!("js update").entered();
        op_state.put(span);
    }
    op_state.put(receiver);

    let mut entity_map = op_state.take::<CrdtContext>();
    let mut renderer_state = op_state.take::<RendererStore>();
    let writers = op_state.take::<CrdtComponentInterfaces>();

    let mut results = match response {
        Some(RendererResponse::Ok(updates)) => {
            let mut results = Vec::new();
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

            // store the updates
            renderer_state.0.update_from(updates);

            results
        }
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
                    );
                    results.push(data);
                }
                GlobalCrdtStateUpdate::Time(time) => {
                    let time_of_day = op_state.borrow_mut::<TimeOfDay>();
                    time_of_day.time = time;
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
