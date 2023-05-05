// Engine module

use bevy::prelude::{debug, info, warn};
use deno_core::{op, OpDecl, OpState};
use std::{cell::RefCell, rc::Rc, sync::mpsc::SyncSender};
use tokio::sync::{broadcast::error::TryRecvError, mpsc::Receiver};

use crate::{
    dcl::{
        crdt::{append_component, put_component},
        interface::crdt_context::CrdtContext,
        CrdtComponentInterfaces, CrdtStore, RendererResponse, SceneElapsedTime, SceneResponse,
    },
    dcl_component::DclReader,
};

use super::ShuttingDown;

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![
        op_crdt_send_to_renderer::decl(),
        op_crdt_recv_from_renderer::decl(),
    ]
}

// receive and process a buffer of crdt messages
#[op(v8)]
fn op_crdt_send_to_renderer(op_state: Rc<RefCell<OpState>>, messages: &[u8]) {
    let mut op_state = op_state.borrow_mut();
    let elapsed_time = op_state.borrow::<SceneElapsedTime>().0;
    let mut entity_map = op_state.take::<CrdtContext>();
    let mut typemap = op_state.take::<CrdtStore>();
    let writers = op_state.take::<CrdtComponentInterfaces>();
    let mut stream = DclReader::new(messages);
    debug!("BATCH len: {}", stream.len());

    // collect commands
    typemap.process_message_stream(&mut entity_map, &writers, &mut stream, true);

    let census = entity_map.take_census();
    typemap.clean_up(&census.died);
    let updates = typemap.take_updates();

    let sender = op_state.borrow_mut::<SyncSender<SceneResponse>>();
    sender
        .send(SceneResponse::Ok(
            entity_map.scene_id,
            census,
            updates,
            SceneElapsedTime(elapsed_time),
        ))
        .expect("failed to send to renderer");

    op_state.put(writers);
    op_state.put(entity_map);
    op_state.put(typemap);
}

#[op(v8)]
async fn op_crdt_recv_from_renderer(op_state: Rc<RefCell<OpState>>) -> Vec<Vec<u8>> {
    let mut receiver = op_state.borrow_mut().take::<Receiver<RendererResponse>>();
    let response = receiver.recv().await;
    op_state.borrow_mut().put(receiver);

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
            results
        }
        None => {
            // channel has been closed, shutdown gracefully
            info!("{}: shutting down", std::thread::current().name().unwrap());
            op_state.borrow_mut().put(ShuttingDown);
            Default::default()
        }
    };

    let mut borrow = op_state.borrow_mut();
    let global_update_receiver = borrow.borrow_mut::<tokio::sync::broadcast::Receiver<Vec<u8>>>();
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

    results
}
