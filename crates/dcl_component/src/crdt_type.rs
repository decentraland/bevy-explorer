use serde::{Deserialize, Serialize};

#[derive(PartialEq, Eq, Clone, Copy, Debug, Serialize, Deserialize)]
pub enum ComponentPosition {
    RootOnly,
    EntityOnly,
    Any,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug, Serialize, Deserialize)]
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
