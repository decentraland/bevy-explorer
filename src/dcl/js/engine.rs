// Engine module

use bevy::prelude::{debug, error, info, warn};
use deno_core::{op, OpDecl, OpState};
use num::{FromPrimitive, ToPrimitive};
use num_derive::{FromPrimitive, ToPrimitive};
use std::{cell::RefCell, rc::Rc, sync::mpsc::SyncSender};
use tokio::sync::mpsc::Receiver;

use crate::{
    dcl::{
        crdt::{growonly::CrdtGOEntry, lww::LWWEntry},
        interface::ComponentPosition,
        CrdtComponentInterfaces, CrdtStore, RendererResponse, SceneResponse, SceneElapsedTime,
    },
    dcl_assert,
    dcl_component::{
        DclReader, DclReaderError, DclWriter, SceneComponentId, SceneCrdtTimestamp, SceneEntityId,
        ToDclWriter,
    },
};

use super::ShuttingDown;

use super::context::SceneSceneContext;

const CRDT_HEADER_SIZE: usize = 8;

#[derive(FromPrimitive, ToPrimitive, Debug)]
pub enum CrdtMessageType {
    PutComponent = 1,
    DeleteComponent = 2,

    DeleteEntity = 3,
    AppendValue = 4,
}

impl ToDclWriter for CrdtMessageType {
    fn to_writer(&self, buf: &mut DclWriter) {
        buf.write_u32(ToPrimitive::to_u32(self).unwrap())
    }
}

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![
        op_crdt_send_to_renderer::decl(),
        op_crdt_recv_from_renderer::decl(),
    ]
}

// handles a single message from the buffer
fn process_message(
    writers: &CrdtComponentInterfaces,
    typemap: &mut CrdtStore,
    entity_map: &mut SceneSceneContext,
    crdt_type: CrdtMessageType,
    stream: &mut DclReader,
) -> Result<(), DclReaderError> {
    match crdt_type {
        CrdtMessageType::PutComponent => {
            let entity = stream.read()?;
            let component = stream.read()?;
            let timestamp = stream.read()?;
            let content_len = stream.read_u32()? as usize;

            debug!("PUT e:{entity:?}, c: {component:?}, timestamp: {timestamp:?}, content len: {content_len}");
            dcl_assert!(content_len == stream.len());

            // check for a writer
            let Some(writer) = writers.0.get(&component) else {
                return Ok(())
            };

            match (writer.position(), entity == SceneEntityId::ROOT) {
                (ComponentPosition::RootOnly, false) | (ComponentPosition::EntityOnly, true) => {
                    warn!("invalid position for component {:?}", component);
                    return Ok(());
                }
                _ => (),
            }

            // create the entity (if not already dead)
            if !entity_map.init(entity) {
                return Ok(());
            }

            // attempt to write (may fail due to a later write)
            typemap.try_update(component, *writer, entity, timestamp, Some(stream));
        }
        CrdtMessageType::DeleteComponent => {
            let entity = stream.read()?;
            let component = stream.read()?;
            let timestamp = stream.read()?;

            // check for a writer
            let Some(writer) = writers.0.get(&component) else {
                return Ok(())
            };

            match (writer.position(), entity == SceneEntityId::ROOT) {
                (ComponentPosition::RootOnly, false) | (ComponentPosition::EntityOnly, true) => {
                    warn!("invalid position for component {:?}", component);
                    return Ok(());
                }
                _ => (),
            }

            // check the entity still lives (don't create here, no need)
            if entity_map.is_dead(entity) {
                return Ok(());
            }

            // attempt to write (may fail due to a later write)
            typemap.try_update(component, *writer, entity, timestamp, None);
        }
        CrdtMessageType::DeleteEntity => {
            let entity = stream.read()?;
            entity_map.kill(entity);
        }
        CrdtMessageType::AppendValue => unimplemented!(),
    }

    Ok(())
}

// receive and process a buffer of crdt messages
#[op(v8)]
fn op_crdt_send_to_renderer(op_state: Rc<RefCell<OpState>>, messages: &[u8]) {
    let mut op_state = op_state.borrow_mut();
    let elapsed_time = op_state.borrow::<SceneElapsedTime>().0;
    let mut entity_map = op_state.take::<SceneSceneContext>();
    let mut typemap = op_state.take::<CrdtStore>();
    let writers = op_state.take::<CrdtComponentInterfaces>();
    let mut stream = DclReader::new(messages);
    debug!("BATCH len: {}", stream.len());

    // collect commands
    while stream.len() > CRDT_HEADER_SIZE {
        let pos = stream.pos();
        let length = stream.read_u32().unwrap() as usize;
        let crdt_type = stream.read_u32().unwrap();

        debug!("[{pos}] crdt_type: {crdt_type}, length: {length}");
        let mut message_stream = stream.take_reader(length.saturating_sub(8));

        match FromPrimitive::from_u32(crdt_type) {
            Some(crdt_type) => {
                if let Err(e) = process_message(
                    &writers,
                    &mut typemap,
                    &mut entity_map,
                    crdt_type,
                    &mut message_stream,
                ) {
                    error!("CRDT Buffer error: {:?}", e);
                };
            }
            None => error!("CRDT Header error: unhandled crdt message type {crdt_type}"),
        }
    }

    let census = entity_map.take_census();
    typemap.clean_up(&census.died);
    let updates = typemap.take_updates();

    let sender = op_state.borrow_mut::<SyncSender<SceneResponse>>();
    sender
        .send(SceneResponse::Ok(entity_map.scene_id, census, updates, SceneElapsedTime(elapsed_time)))
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
