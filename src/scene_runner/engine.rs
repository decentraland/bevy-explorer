// Engine module

use crate::scene_runner::crdt::CrdtComponentInterfaces;

use super::{
    crdt::CrdtInterfacesMap, EngineResponseList, SceneComponentId, SceneContext, SceneEntityId,
};
use bevy::prelude::{debug, error};
use deno_core::{op, OpDecl, OpState};
use num::FromPrimitive;
use num_derive::FromPrimitive;
use std::{
    cell::{RefCell, RefMut},
    mem::MaybeUninit,
    rc::Rc,
};

const CRDT_HEADER_SIZE: u64 = 8;

#[derive(FromPrimitive, Debug)]
pub enum CrdtMessageType {
    PutComponent = 1,
    DeleteComponent = 2,

    DeleteEntity = 3,
    AppendValue = 4,
}

// buffer format helpers

/// `MaybeUninit::array_assume_init` is not stable.
#[inline]
pub(crate) unsafe fn maybe_ununit_array_assume_init<T, const N: usize>(
    array: [MaybeUninit<T>; N],
) -> [T; N] {
    // SAFETY:
    // * The caller guarantees that all elements of the array are initialized
    // * `MaybeUninit<T>` and T are guaranteed to have the same layout
    // * `MaybeUninit` does not drop, so there are no double-frees
    // And thus the conversion is safe
    (&array as *const _ as *const [T; N]).read()
}

pub trait ReadDclFormat {
    /// Read little-endian 32-bit integer
    fn read_be_u32(&mut self) -> protobuf::Result<u32>;
    fn read_be_float(&mut self) -> protobuf::Result<f32>;
}

impl<'de> ReadDclFormat for protobuf::CodedInputStream<'de> {
    fn read_be_u32(&mut self) -> protobuf::Result<u32> {
        let mut bytes = [MaybeUninit::uninit(); 4];
        self.read_exact(&mut bytes)?;
        // SAFETY: `read_exact` guarantees that the buffer is filled.
        let bytes = unsafe { maybe_ununit_array_assume_init(bytes) };
        Ok(u32::from_be_bytes(bytes))
    }

    fn read_be_float(&mut self) -> protobuf::Result<f32> {
        let bits = self.read_be_u32()?;
        Ok(f32::from_bits(bits))
    }
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
    stream: &mut protobuf::CodedInputStream,
) -> Result<(), protobuf::Error> {
    match crdt_type {
        CrdtMessageType::PutComponent => {
            let entity = SceneEntityId(stream.read_be_u32()?);
            let component = SceneComponentId(stream.read_be_u32()?);
            let timestamp = stream.read_be_u32()?;
            let content_len = stream.read_be_u32()?;

            debug!("PUT e:{entity:?}, c: {component:?}, timestamp: {timestamp}, content len: {content_len}");
            assert_eq!((content_len as u64), stream.bytes_until_limit());

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
            let entity = SceneEntityId(stream.read_be_u32()?);
            let component = SceneComponentId(stream.read_be_u32()?);
            let timestamp = stream.read_be_u32()?;

            // check for a writer
            let Some(writer) = writers.get(&component) else {
                return Ok(())
            };

            // check the entity still lives (don't create here, no need)
            if !entity_map.is_live(entity) {
                return Ok(());
            }

            // attempt to write (may fail due to a later write)
            if !writer.update_crdt(op_state, entity, timestamp, None)? {
                return Ok(());
            }
        }
        CrdtMessageType::DeleteEntity => {
            let entity = SceneEntityId(stream.read_be_u32()?);
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
    let mut stream = protobuf::CodedInputStream::from_bytes(messages);
    stream.push_limit(messages.len() as u64).unwrap();
    let message_limit = stream.bytes_until_limit();
    debug!("BATCH limit: {}, len: {}", message_limit, messages.len());

    // collect commands
    while stream.bytes_until_limit() > CRDT_HEADER_SIZE {
        let pos = stream.pos();
        let length = stream.read_be_u32().unwrap();
        let crdt_type = stream.read_be_u32().unwrap();

        debug!("[{pos}] crdt_type: {crdt_type}, length: {length}");
        if let Err(e) = stream.push_limit(length.saturating_sub(8) as u64) {
            error!("CRDT Header error: failed to set limit {}: {}", length, e);
        };

        match FromPrimitive::from_u32(crdt_type) {
            Some(crdt_type) => {
                if let Err(e) = process_message(
                    &mut op_state,
                    writers,
                    &mut entity_map,
                    crdt_type,
                    &mut stream,
                ) {
                    error!("CRDT Buffer error: {}", e);
                };
            }
            None => error!("CRDT Header error: unhandled crdt message type {crdt_type}"),
        }

        if let Err(e) = stream.skip_raw_bytes(stream.bytes_until_limit().try_into().unwrap()) {
            error!("CRDT Header error: failed to skip bytes: {}", e);
        }
        stream.pop_limit(message_limit);
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
