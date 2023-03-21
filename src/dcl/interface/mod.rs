// interface structs shared between renderer and js

use bevy::utils::HashMap;

use crate::dcl_component::{DclReader, SceneComponentId, SceneCrdtTimestamp, SceneEntityId};

use super::crdt::lww::CrdtLWWState;

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum ComponentPosition {
    RootOnly,
    EntityOnly,
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum CrdtType {
    LWW(ComponentPosition),
}

impl CrdtType {
    pub fn position(&self) -> ComponentPosition {
        match self {
            CrdtType::LWW(pos) => *pos,
        }
    }
}

pub struct CrdtComponentInterfaces(pub HashMap<SceneComponentId, CrdtType>);

#[derive(Default, Debug)]
pub struct CrdtStore {
    pub lww: HashMap<SceneComponentId, CrdtLWWState>,
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
            CrdtType::LWW(_) => self
                .lww
                .entry(component_id)
                .or_insert_with(CrdtLWWState::default)
                .try_update(entity, new_timestamp, maybe_new_data),
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

        CrdtStore { lww }
    }
}
