// interface structs shared between renderer and js
pub mod lww_interface;

use bevy::utils::HashMap;

use crate::dcl_component::SceneComponentId;

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

#[derive(Default)]
pub struct CrdtStore {
    pub lww: HashMap<SceneComponentId, CrdtLWWState>,
}
