// interface structs shared between renderer and js
pub mod lww_interface;

use std::{
    any::{Any, TypeId},
    collections::BTreeMap,
    sync::Arc,
};

use bevy::{ecs::system::EntityCommands, prelude::Resource, utils::HashMap};

use crate::dcl_component::{
    DclReader, DclReaderError, SceneComponentId, SceneCrdtTimestamp, SceneEntityId,
};

// trait for enacpsulating the processing of a crdt message
pub trait CrdtInterface {
    // update the crdt store for this data
    // returns true if the store was modified
    fn update_crdt(
        &self,
        target: &mut CrdtStore,
        entity: SceneEntityId,
        timestamp: SceneCrdtTimestamp,
        data: Option<&mut DclReader>,
    ) -> Result<bool, DclReaderError>;
    // extract any updates from the source into the target, and flush the source updates
    fn take_updates(&self, source: &mut CrdtStore, target: &mut CrdtStore);
    // push updates onto a bevy entity
    fn updates_to_entity(&self, type_map: &mut CrdtStore, commands: &mut EntityCommands);
}

pub type CrdtInterfacesMap = HashMap<SceneComponentId, Box<dyn CrdtInterface + Send + Sync>>;

// vtables for buffer (de)serialization
#[derive(Resource, Clone, Default)]
pub struct CrdtComponentInterfaces(pub Arc<CrdtInterfacesMap>);

#[derive(Default)]
pub struct CrdtStore(BTreeMap<TypeId, Box<dyn Any + Send + Sync>>);
impl CrdtStore {
    pub fn insert<T: Send + Sync + 'static>(&mut self, value: T) {
        self.0.insert(std::any::TypeId::of::<T>(), Box::new(value));
    }
    pub fn borrow_mut<T: Send + Sync + 'static>(&mut self) -> Option<&mut T> {
        self.0
            .get_mut(&std::any::TypeId::of::<T>())
            .and_then(|b| b.downcast_mut())
    }
    pub fn take<T: Send + Sync + 'static>(&mut self) -> Option<T> {
        self.0
            .remove(&std::any::TypeId::of::<T>())
            .and_then(|b| b.downcast().ok())
            .map(|b| *b)
    }
}
