// interface structs shared between renderer and js.
// note that take_updates assumes a single lossless reader will be synchronised -
// it resets internal "updated" markers (for lww) and removes unneeded data (for go)

use bevy::utils::{HashMap, HashSet};

use crate::dcl_component::{DclReader, SceneComponentId, SceneCrdtTimestamp, SceneEntityId};

use super::crdt::{growonly::CrdtGOState, lww::CrdtLWWState};

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum ComponentPosition {
    RootOnly,
    EntityOnly,
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum CrdtType {
    LWW(ComponentPosition),
    GO(ComponentPosition),
}

impl CrdtType {
    pub const LWW_ROOT: CrdtType = CrdtType::LWW(ComponentPosition::RootOnly);
    pub const LWW_ENT: CrdtType = CrdtType::LWW(ComponentPosition::EntityOnly);

    pub fn position(&self) -> ComponentPosition {
        match self {
            CrdtType::LWW(pos) => *pos,
            CrdtType::GO(pos) => *pos,
        }
    }
}

pub struct CrdtComponentInterfaces(pub HashMap<SceneComponentId, CrdtType>);

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
}
