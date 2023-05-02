// interface structs shared between renderer and js.
// note that take_updates assumes a single lossless reader will be synchronised -
// it resets internal "updated" markers (for lww) and removes unneeded data (for go)

use bevy::{
    prelude::{debug, error, warn},
    utils::{HashMap, HashSet},
};
use num::{FromPrimitive, ToPrimitive};
use num_derive::{FromPrimitive, ToPrimitive};

use crate::{
    dcl_assert,
    dcl_component::{
        DclReader, DclReaderError, DclWriter, SceneComponentId, SceneCrdtTimestamp, SceneEntityId,
        ToDclWriter,
    },
};

use self::crdt_context::CrdtContext;

use super::crdt::{growonly::CrdtGOState, lww::CrdtLWWState};

pub mod crdt_context;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum ComponentPosition {
    RootOnly,
    EntityOnly,
    Any,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum CrdtType {
    LWW(ComponentPosition),
    GO(ComponentPosition),
}

impl CrdtType {
    pub const LWW_ROOT: CrdtType = CrdtType::LWW(ComponentPosition::RootOnly);
    pub const LWW_ENT: CrdtType = CrdtType::LWW(ComponentPosition::EntityOnly);
    pub const LWW_ANY: CrdtType = CrdtType::LWW(ComponentPosition::Any);
    pub const GO_ENT: CrdtType = CrdtType::GO(ComponentPosition::EntityOnly);
    pub const GO_ANY: CrdtType = CrdtType::GO(ComponentPosition::Any);

    pub fn position(&self) -> ComponentPosition {
        match self {
            CrdtType::LWW(pos) => *pos,
            CrdtType::GO(pos) => *pos,
        }
    }
}

#[derive(Default)]
pub struct CrdtComponentInterfaces(pub HashMap<SceneComponentId, CrdtType>);

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

#[derive(Default, Debug)]
pub struct CrdtStore {
    pub lww: HashMap<SceneComponentId, CrdtLWWState>,
    pub go: HashMap<SceneComponentId, CrdtGOState>,
}

impl CrdtStore {
    pub fn try_update(
        &mut self,
        component_id: SceneComponentId,
        crdt_type: CrdtType,
        entity: SceneEntityId,
        new_timestamp: SceneCrdtTimestamp,
        maybe_new_data: Option<&mut DclReader>,
    ) -> bool {
        match crdt_type {
            CrdtType::LWW(_) => self.lww.entry(component_id).or_default().try_update(
                entity,
                new_timestamp,
                maybe_new_data,
            ),
            CrdtType::GO(_) => {
                self.force_update(component_id, crdt_type, entity, maybe_new_data);
                true
            }
        }
    }

    pub fn force_update(
        &mut self,
        component_id: SceneComponentId,
        crdt_type: CrdtType,
        entity: SceneEntityId,
        maybe_new_data: Option<&mut DclReader>,
    ) {
        match crdt_type {
            CrdtType::LWW(_) => self
                .lww
                .entry(component_id)
                .or_insert_with(CrdtLWWState::default)
                .force_update(entity, maybe_new_data),
            CrdtType::GO(_) => self
                .go
                .entry(component_id)
                .or_default()
                .append(entity, maybe_new_data.unwrap()),
        }
    }

    pub fn clean_up(&mut self, dead: &HashSet<SceneEntityId>) {
        for state in self.lww.values_mut() {
            for id in dead {
                state.last_write.remove(id);
                state.updates.remove(id);
            }
        }
        for state in self.go.values_mut() {
            for id in dead {
                state.0.remove(id);
            }
        }
    }

    pub fn take_updates(&mut self) -> CrdtStore {
        let lww =
            self.lww.iter_mut().map(|(component_id, state)| {
                (
                    *component_id,
                    CrdtLWWState {
                        last_write: HashMap::from_iter(state.updates.iter().map(|update| {
                            (*update, state.last_write.get(update).unwrap().clone())
                        })),
                        updates: std::mem::take(&mut state.updates),
                    },
                )
            });
        let lww = HashMap::from_iter(lww);

        let go = std::mem::take(&mut self.go);
        CrdtStore { lww, go }
    }

    // handles a single message from the buffer
    fn process_message(
        &mut self,
        writers: &CrdtComponentInterfaces,
        entity_map: &mut CrdtContext,
        crdt_type: CrdtMessageType,
        stream: &mut DclReader,
        filter_components: bool,
    ) -> Result<(), DclReaderError> {
        match crdt_type {
            CrdtMessageType::PutComponent | CrdtMessageType::AppendValue => {
                let default_writer = match crdt_type {
                    CrdtMessageType::PutComponent => &CrdtType::LWW_ANY,
                    CrdtMessageType::AppendValue => &CrdtType::GO_ANY,
                    _ => unreachable!(),
                };

                let entity = stream.read()?;
                let component = stream.read()?;
                let timestamp = stream.read()?;
                let content_len = stream.read_u32()? as usize;

                debug!("PUT e:{entity:?}, c: {component:?}, timestamp: {timestamp:?}, content len: {content_len}");
                dcl_assert!(content_len == stream.len());

                // check for a writer
                let writer = match writers.0.get(&component) {
                    Some(writer) => writer,
                    None => {
                        if filter_components {
                            return Ok(());
                        }

                        // if we're not filtering this must be a user component
                        default_writer
                    }
                };

                if core::mem::discriminant(writer) != core::mem::discriminant(default_writer) {
                    warn!("received a {crdt_type:?} message for a {writer:?} component: {component:?}");
                    return Ok(());
                }

                match (writer.position(), entity == SceneEntityId::ROOT) {
                    (ComponentPosition::RootOnly, false)
                    | (ComponentPosition::EntityOnly, true) => {
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
                self.try_update(component, *writer, entity, timestamp, Some(stream));
            }
            CrdtMessageType::DeleteComponent => {
                let entity = stream.read()?;
                let component = stream.read()?;
                let timestamp = stream.read()?;

                // check for a writer
                let writer = match writers.0.get(&component) {
                    Some(writer) => writer,
                    None => {
                        if filter_components {
                            return Ok(());
                        }

                        // if we're not filtering this must be a user component
                        &CrdtType::GO_ANY
                    }
                };

                if !matches!(writer, CrdtType::LWW(_)) {
                    warn!("received a LWW message for a GO component: {component:?}");
                    return Ok(());
                }

                match (writer.position(), entity == SceneEntityId::ROOT) {
                    (ComponentPosition::RootOnly, false)
                    | (ComponentPosition::EntityOnly, true) => {
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
                self.try_update(component, *writer, entity, timestamp, None);
            }
            CrdtMessageType::DeleteEntity => {
                let entity = stream.read()?;
                entity_map.kill(entity);
            }
        }

        Ok(())
    }

    pub fn process_message_stream(
        &mut self,
        entity_map: &mut CrdtContext,
        writers: &CrdtComponentInterfaces,
        stream: &mut DclReader,
        filter_components: bool,
    ) {
        // collect commands
        while stream.len() > CRDT_HEADER_SIZE {
            let pos = stream.pos();
            let length = stream.read_u32().unwrap() as usize;
            let crdt_type = stream.read_u32().unwrap();

            debug!("[{pos}] crdt_type: {crdt_type}, length: {length}");
            let mut message_stream = stream.take_reader(length.saturating_sub(8));

            match FromPrimitive::from_u32(crdt_type) {
                Some(crdt_type) => {
                    if let Err(e) = self.process_message(
                        writers,
                        entity_map,
                        crdt_type,
                        &mut message_stream,
                        filter_components,
                    ) {
                        error!("CRDT Buffer error: {:?}", e);
                    };
                }
                None => error!("CRDT Header error: unhandled crdt message type {crdt_type}"),
            }
        }
    }
}
