use std::{collections::HashMap, sync::Arc};

use anyhow::anyhow;
use bevy::prelude::Resource;

use crate::{CrdtType, SceneComponentId};

/// Decode CRDT bytes into a pretty-printed JSON string.
pub type InspectFn = Arc<dyn Fn(&[u8]) -> anyhow::Result<String> + Send + Sync>;
/// Encode a JSON string into CRDT bytes.
pub type WriteFn = Arc<dyn Fn(&str) -> anyhow::Result<Vec<u8>> + Send + Sync>;

pub struct ComponentNameEntry {
    pub id: SceneComponentId,
    pub crdt_type: CrdtType,
    pub inspect: InspectFn,
    /// None for inspect-only (engine→scene only) components.
    pub write: Option<WriteFn>,
}

#[derive(Resource, Default)]
pub struct ComponentNameRegistry {
    by_name: HashMap<String, ComponentNameEntry>,
    by_id: HashMap<SceneComponentId, String>,
}

impl ComponentNameRegistry {
    pub fn register(
        &mut self,
        name: String,
        id: SceneComponentId,
        crdt_type: CrdtType,
        inspect: InspectFn,
        write: Option<WriteFn>,
    ) {
        self.by_id.insert(id, name.clone());
        self.by_name.insert(
            name,
            ComponentNameEntry {
                id,
                crdt_type,
                inspect,
                write,
            },
        );
    }

    pub fn get_by_name(&self, name: &str) -> Option<&ComponentNameEntry> {
        self.by_name.get(name)
    }

    pub fn get_by_id(&self, id: SceneComponentId) -> Option<&ComponentNameEntry> {
        let name = self.by_id.get(&id)?;
        self.by_name.get(name)
    }

    pub fn name_for_id(&self, id: SceneComponentId) -> Option<&str> {
        self.by_id.get(&id).map(|s| s.as_str())
    }

    pub fn all_names(&self) -> impl Iterator<Item = &str> {
        self.by_name.keys().map(|s| s.as_str())
    }

    pub fn all_id_name_pairs(&self) -> impl Iterator<Item = (SceneComponentId, &str)> {
        self.by_id.iter().map(|(id, name)| (*id, name.as_str()))
    }
}

/// Derive the PascalCase component name from a Rust type name.
/// e.g. `dcl_component::proto_components::sdk::components::PbMeshRenderer` → `MeshRenderer`
pub fn derive_component_name<D>() -> String {
    let full_name = std::any::type_name::<D>();
    let short = full_name.split("::").last().unwrap_or(full_name);
    short.strip_prefix("Pb").unwrap_or(short).to_string()
}

/// Build inspect/write closures for a prost + serde type.
pub fn make_proto_closures<D>() -> (InspectFn, WriteFn)
where
    D: prost::Message + serde::Serialize + serde::de::DeserializeOwned + Default,
{
    let inspect = Arc::new(|bytes: &[u8]| {
        let msg = D::decode(bytes).map_err(|e| anyhow!("{e}"))?;
        serde_json::to_string_pretty(&msg).map_err(|e| anyhow!("{e}"))
    });
    let write = Arc::new(|json: &str| {
        let msg: D = serde_json::from_str(json).map_err(|e| anyhow!("{e}"))?;
        let mut buf = Vec::new();
        prost::Message::encode(&msg, &mut buf).map_err(|e| anyhow!("{e}"))?;
        Ok(buf)
    });
    (inspect, write)
}
