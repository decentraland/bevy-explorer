// Engine module

use bevy::prelude::{debug, info};
use deno_core::{op, OpDecl, OpState};
use std::{cell::RefCell, rc::Rc, sync::mpsc::SyncSender};
use tokio::sync::mpsc::Receiver;

use crate::{
    dcl::{
        crdt::{growonly::CrdtGOEntry, lww::LWWEntry},
        interface::{crdt_context::CrdtContext, CrdtMessageType},
        CrdtComponentInterfaces, CrdtStore, RendererResponse, SceneElapsedTime, SceneResponse,
    },
    dcl_component::{DclReader, DclWriter, SceneComponentId, SceneCrdtTimestamp, SceneEntityId},
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

fn put_component(
    entity_id: &SceneEntityId,
    component_id: &SceneComponentId,
    entry: &LWWEntry,
) -> Vec<u8> {
    let content_len = entry.data.len();
    let length = content_len + 12 + if entry.is_some { 4 } else { 0 } + 8;

    let mut buf = Vec::with_capacity(length);
    let mut writer = DclWriter::new(&mut buf);
    writer.write_u32(length as u32);

    if entry.is_some {
        writer.write(&CrdtMessageType::PutComponent);
    } else {
        writer.write(&CrdtMessageType::DeleteComponent);
    }

    writer.write(entity_id);
    writer.write(component_id);
    writer.write(&entry.timestamp);

    if entry.is_some {
        writer.write_u32(content_len as u32);
        writer.write_raw(&entry.data)
    }

    buf
}

fn append_component(
    entity_id: &SceneEntityId,
    component_id: &SceneComponentId,
    entry: &CrdtGOEntry,
) -> Vec<u8> {
    let content_len = entry.data.len();
    let length = content_len + 12 + 4 + 8;

    let mut buf = Vec::with_capacity(length);
    let mut writer = DclWriter::new(&mut buf);
    writer.write_u32(length as u32);
    writer.write(&CrdtMessageType::AppendValue);

    writer.write(entity_id);
    writer.write(component_id);
    writer.write(&SceneCrdtTimestamp(0));

    writer.write_u32(content_len as u32);
    writer.write_raw(&entry.data);

    buf
}

#[op(v8)]
async fn op_crdt_recv_from_renderer(op_state: Rc<RefCell<OpState>>) -> Vec<Vec<u8>> {
    let mut receiver = op_state.borrow_mut().take::<Receiver<RendererResponse>>();
    let response = receiver.recv().await;
    op_state.borrow_mut().put(receiver);

    let results = match response {
        Some(RendererResponse::Ok(updates)) => {
            let mut results = Vec::new();
            // TODO: consider writing directly into a v8 buffer
            for (component_id, lww) in updates.lww.iter() {
                for (entity_id, data) in lww.last_write.iter() {
                    results.push(put_component(entity_id, component_id, data));
                }
            }
            for (component_id, go) in updates.go.iter() {
                for (entity_id, data) in go.0.iter() {
                    for item in data.iter() {
                        results.push(append_component(entity_id, component_id, item));
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

    results
}
