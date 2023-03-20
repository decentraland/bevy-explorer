// Engine module

use bevy::prelude::{debug, error, info};
use deno_core::{op, OpDecl, OpState};
use num::FromPrimitive;
use num_derive::{FromPrimitive, ToPrimitive};
use std::{cell::RefCell, rc::Rc, sync::mpsc::SyncSender};
use tokio::sync::mpsc::Receiver;

use crate::{
    dcl::{
        interface::{CrdtComponentInterfaces, CrdtInterfacesMap, CrdtStore},
        RendererResponse, SceneResponse,
    },
    dcl_assert,
    dcl_component::{DclReader, DclReaderError},
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

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![
        op_crdt_send_to_renderer::decl(),
        op_crdt_recv_from_renderer::decl(),
    ]
}

// handles a single message from the buffer
fn process_message(
    writers: &CrdtInterfacesMap,
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
            let Some(writer) = writers.get(&component) else {
                return Ok(())
            };

            // create the entity (if not already dead)
            if !entity_map.init(entity) {
                return Ok(());
            }

            // attempt to write (may fail due to a later write)
            if !writer.update_crdt(typemap, entity, timestamp, Some(stream))? {
                return Ok(());
            }
        }
        CrdtMessageType::DeleteComponent => {
            let entity = stream.read()?;
            let component = stream.read()?;
            let timestamp = stream.read()?;

            // check for a writer
            let Some(writer) = writers.get(&component) else {
                return Ok(())
            };

            // check the entity still lives (don't create here, no need)
            if entity_map.is_dead(entity) {
                return Ok(());
            }

            // attempt to write (may fail due to a later write)
            if !writer.update_crdt(typemap, entity, timestamp, None)? {
                return Ok(());
            }
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
                    &writers.0,
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

    let mut updates = CrdtStore::default();
    for writer in writers.0.values() {
        writer.take_updates(&mut typemap, &mut updates);
    }
    let census = entity_map.take_census();

    let sender = op_state.borrow_mut::<SyncSender<SceneResponse>>();
    sender
        .send(SceneResponse::Ok(entity_map.scene_id, census, updates))
        .expect("failed to send to renderer");

    op_state.put(writers);
    op_state.put(entity_map);
    op_state.put(typemap);
}

#[op(v8)]
async fn op_crdt_recv_from_renderer(op_state: Rc<RefCell<OpState>>) -> Vec<()> {
    let mut receiver = op_state.borrow_mut().take::<Receiver<RendererResponse>>();
    let response = receiver.recv().await;
    op_state.borrow_mut().put(receiver);

    match response {
        Some(_) => Default::default(),
        None => {
            // channel has been closed, shutdown gracefully
            info!("{}: shutting down", std::thread::current().name().unwrap());
            op_state.borrow_mut().put(ShuttingDown);
            Default::default()
        }
    }
}
