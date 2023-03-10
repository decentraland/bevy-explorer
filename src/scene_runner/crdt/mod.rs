use std::{cell::RefMut, sync::Arc};

use bevy::{
    ecs::system::EntityCommands,
    prelude::{App, Component, IntoSystemConfig, Resource},
    utils::HashMap,
};
use deno_core::OpState;

use super::{
    engine::{DclReader, DclReaderError},
    SceneComponentId, SceneCrdtTimestamp, SceneEntityId, SceneSets,
};

pub mod lww;

use self::lww::{process_crdt_lww_updates, CrdtLWWInterface};

// trait for enacpsulating the processing of a crdt message
pub trait CrdtInterface {
    fn update_crdt(
        &self,
        op_state: &mut RefMut<OpState>,
        entity: SceneEntityId,
        timestamp: SceneCrdtTimestamp,
        data: Option<&mut DclReader>,
    ) -> Result<bool, DclReaderError>;
    fn claim_crdt(&self, op_state: &mut RefMut<OpState>, target: &mut EntityCommands);
}

// trait to build an object from a buffer stream
pub trait FromDclReader: Send + Sync + 'static {
    fn from_proto(buf: &mut DclReader) -> Result<Self, DclReaderError>
    where
        Self: Sized;
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
        self.add_system(process_crdt_lww_updates::<T>.in_set(SceneSets::HandleOutput));
    }
}
