// Engine module
use bevy::log::{debug, info, warn};

#[cfg(feature = "span_scene_loop")]
use bevy::log::{info_span, tracing::span::EnteredSpan};

use std::{cell::RefCell, rc::Rc, sync::Arc};
use tokio::sync::{broadcast::error::TryRecvError, Mutex};

use crate::{
    crdt::{append_component, put_component},
    interface::crdt_context::CrdtContext,
    js::{CommunicatedWithRenderer, RendererStore, SceneResponseSender, ShuttingDown},
    CrdtComponentInterfaces, CrdtStore, RendererResponse, RpcCalls, SceneElapsedTime,
    SceneLogMessage, SceneResponse,
};
use dcl_component::DclReader;

use super::State;

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
    let receiver = op_state
        .borrow_mut()
        .borrow_mut::<Arc<Mutex<tokio::sync::mpsc::Receiver<RendererResponse>>>>()
        .clone();
    let response = receiver.lock().await.recv().await;

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
    };

    let mut global_update_receiver = op_state.take::<tokio::sync::broadcast::Receiver<Vec<u8>>>();
    loop {
        match global_update_receiver.try_recv() {
            Ok(next) => {
                let mut stream = DclReader::new(&next);
                renderer_state.0.process_message_stream(
                    &mut entity_map,
                    &writers,
                    &mut stream,
                    false,
                );
                results.push(next);
            }
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
