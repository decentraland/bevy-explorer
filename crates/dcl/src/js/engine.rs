// Engine module
use bevy::{
    utils::tracing::span::EnteredSpan,
    utils::tracing::{debug, info, info_span, warn},
};
use deno_core::{op2, OpDecl, OpState};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{mpsc::SyncSender, Arc},
};
use tokio::sync::{broadcast::error::TryRecvError, mpsc::Receiver, Mutex};

use crate::{
    crdt::{append_component, put_component},
    interface::crdt_context::CrdtContext,
    js::{CommunicatedWithRenderer, RendererStore, ShuttingDown},
    CrdtComponentInterfaces, CrdtStore, RendererResponse, RpcCalls, SceneElapsedTime,
    SceneLogMessage, SceneResponse,
};
use dcl_component::DclReader;

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![op_crdt_send_to_renderer(), op_crdt_recv_from_renderer()]
}

// receive and process a buffer of crdt messages
#[op2(fast)]
fn op_crdt_send_to_renderer(op_state: Rc<RefCell<OpState>>, #[arraybuffer] messages: &[u8]) {
    crdt_send_to_renderer(op_state, messages)
}

pub fn crdt_send_to_renderer(op_state: Rc<RefCell<OpState>>, messages: &[u8]) {
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

    let sender = op_state.borrow_mut::<SyncSender<SceneResponse>>();
    sender
        .send(SceneResponse::Ok(
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

#[op2(async)]
#[serde]
async fn op_crdt_recv_from_renderer(op_state: Rc<RefCell<OpState>>) -> Vec<Vec<u8>> {
    let span = op_state.borrow_mut().try_take::<EnteredSpan>();
    drop(span); // don't hold it over the await point so we get a clearer view of when js is running

    debug!("op_crdt_recv_from_renderer");
    let receiver = op_state
        .borrow_mut()
        .borrow_mut::<Arc<Mutex<Receiver<RendererResponse>>>>()
        .clone();
    let response = receiver.lock().await.recv().await;

    let mut op_state = op_state.borrow_mut();
    let span = info_span!("js update").entered();
    op_state.put(span);
    op_state.put(receiver);

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
            let renderer_state = match op_state.try_borrow_mut::<RendererStore>() {
                Some(state) => state,
                None => {
                    op_state.put(RendererStore(Default::default()));
                    op_state.borrow_mut::<RendererStore>()
                }
            };
            renderer_state.0.update_from(updates);

            results
        }
        None => {
            // channel has been closed, shutdown gracefully
            info!("{}: shutting down", std::thread::current().name().unwrap());
            op_state.put(ShuttingDown);
            Default::default()
        }
    };

    let global_update_receiver = op_state.borrow_mut::<tokio::sync::broadcast::Receiver<Vec<u8>>>();
    loop {
        match global_update_receiver.try_recv() {
            Ok(next) => results.push(next),
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Lagged(_)) => (), // continue on with whatever we can still get
            Err(TryRecvError::Closed) => {
                warn!("global receiver shut down");
                break;
            }
        }
    }

    op_state.put(CommunicatedWithRenderer);

    results
}
