// Engine module

use bevy::prelude::{debug, error};
use deno_core::{op, OpDecl, OpState};
use num::FromPrimitive;
use num_derive::FromPrimitive;
use std::{
    cell::{RefCell, RefMut},
    rc::Rc,
};

use crate::{
    crdt::{CrdtComponentInterfaces, CrdtInterfacesMap},
    dcl_component::{DclReader, DclReaderError},
    scene_runner::EngineResponseList,
};

use super::SceneContext;

const CRDT_HEADER_SIZE: usize = 8;

#[derive(FromPrimitive, Debug)]
pub enum CrdtMessageType {
    PutComponent = 1,
    DeleteComponent = 2,

    DeleteEntity = 3,
    AppendValue = 4,
}

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![op_crdt_send_to_renderer::decl()]
}

// handles a single message from the buffer
fn process_message(
    op_state: &mut RefMut<OpState>,
    writers: &CrdtInterfacesMap,
    entity_map: &mut SceneContext,
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
            assert_eq!(content_len, stream.len());

            // check for a writer
            let Some(writer) = writers.get(&component) else {
                return Ok(())
            };

            // create the entity (if not already dead)
            if !entity_map.init(entity) {
                return Ok(());
            }

            // attempt to write (may fail due to a later write)
            if !writer.update_crdt(op_state, entity, timestamp, Some(stream))? {
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
            if !writer.update_crdt(op_state, entity, timestamp, None)? {
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
fn op_crdt_send_to_renderer(op_state: Rc<RefCell<OpState>>, messages: &[u8]) -> Vec<String> {
    let mut op_state = op_state.borrow_mut();
    let mut entity_map = op_state.take::<SceneContext>();
    let writers = op_state.borrow::<CrdtComponentInterfaces>().clone();
    let writers = writers.0.as_ref();
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
                    &mut op_state,
                    writers,
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

    op_state.put(entity_map);

    // return responses
    let responses = op_state.borrow::<EngineResponseList>();
    responses
        .0
        .iter()
        .map(|response| serde_json::to_string(response).unwrap())
        .collect()
}
