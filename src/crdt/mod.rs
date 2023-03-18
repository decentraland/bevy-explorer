use std::{
    any::{Any, TypeId},
    collections::BTreeMap,
    sync::Arc,
};

use bevy::{
    ecs::system::EntityCommands,
    prelude::{App, Component, IntoSystemConfig, Resource},
    utils::HashMap,
};

pub mod lww;

use crate::{
    dcl_component::{
        DclReader, DclReaderError, FromDclReader, SceneComponentId, SceneCrdtTimestamp,
        SceneEntityId,
    },
    scene_runner::{SceneLoopSchedule, SceneLoopSets},
};

use self::lww::{process_crdt_lww_updates, CrdtLWWInterface};

#[derive(Default)]
pub struct TypeMap(BTreeMap<TypeId, Box<dyn Any + Send + Sync>>);
impl TypeMap {
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

// trait for enacpsulating the processing of a crdt message
pub trait CrdtInterface {
    fn update_crdt(
        &self,
        target: &mut TypeMap,
        entity: SceneEntityId,
        timestamp: SceneCrdtTimestamp,
        data: Option<&mut DclReader>,
    ) -> Result<bool, DclReaderError>;
    fn take_updates(&self, source: &mut TypeMap, target: &mut TypeMap);
    fn updates_to_entity(&self, type_map: &mut TypeMap, commands: &mut EntityCommands);
}

pub type CrdtInterfacesMap = HashMap<SceneComponentId, Box<dyn CrdtInterface + Send + Sync>>;

// vtables for buffer (de)serialization
#[derive(Resource, Clone, Default)]
pub struct CrdtComponentInterfaces(pub Arc<CrdtInterfacesMap>);

// a helper to automatically apply engine component updates
pub trait AddCrdtInterfaceExt {
    fn add_crdt_lww_interface<T: FromDclReader>(&mut self, id: SceneComponentId);

    fn add_crdt_lww_component<T: FromDclReader + Component + std::fmt::Debug>(
        &mut self,
        id: SceneComponentId,
    );
}

impl AddCrdtInterfaceExt for App {
    fn add_crdt_lww_interface<T: FromDclReader>(&mut self, id: SceneComponentId) {
        // store a writer
        let mut res = self.world.resource_mut::<CrdtComponentInterfaces>();
        let inner = std::mem::take(&mut res.0);
        let Ok(mut inner) = Arc::try_unwrap(inner) else { panic!() };
        inner.insert(id, Box::<CrdtLWWInterface<T>>::default());
        res.0 = Arc::new(inner);
    }

    fn add_crdt_lww_component<T: FromDclReader + Component + std::fmt::Debug>(
        &mut self,
        id: SceneComponentId,
    ) {
        self.add_crdt_lww_interface::<T>(id);
        // add a system to process the update
        self.world
            .resource_mut::<SceneLoopSchedule>()
            .0
            .add_system(process_crdt_lww_updates::<T>.in_set(SceneLoopSets::UpdateWorld));
    }
}
