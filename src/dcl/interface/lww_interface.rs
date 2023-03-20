// interface managing data transfer for LWW components

use std::marker::PhantomData;

use bevy::{ecs::system::EntityCommands, utils::HashMap};

use crate::{
    dcl::crdt::lww::CrdtLWWState,
    dcl_component::{DclReader, DclReaderError, FromDclReader, SceneCrdtTimestamp, SceneEntityId},
    scene_runner::update_world::CrdtLWWStateComponent,
};

use super::{CrdtInterface, CrdtStore};

pub struct CrdtLWWInterface<T: FromDclReader> {
    _marker: PhantomData<T>,
}

impl<T: FromDclReader> Default for CrdtLWWInterface<T> {
    fn default() -> Self {
        Self {
            _marker: Default::default(),
        }
    }
}

impl<T: FromDclReader> CrdtInterface for CrdtLWWInterface<T> {
    fn update_crdt(
        &self,
        target: &mut CrdtStore,
        entity: SceneEntityId,
        new_timestamp: SceneCrdtTimestamp,
        maybe_new_data: Option<&mut DclReader>,
    ) -> Result<bool, DclReaderError> {
        // create state if required
        let state = match target.borrow_mut::<CrdtLWWState<T>>() {
            Some(state) => state,
            None => {
                target.insert(CrdtLWWState::<T>::default());
                target.borrow_mut().unwrap()
            }
        };

        state.update(entity, new_timestamp, maybe_new_data)
    }

    fn take_updates(&self, source: &mut CrdtStore, target: &mut CrdtStore) {
        if let Some(state) = source.borrow_mut::<CrdtLWWState<T>>() {
            let udpated_state = CrdtLWWState::<T> {
                last_write: HashMap::from_iter(
                    state
                        .updates
                        .iter()
                        .map(|update| (*update, state.last_write.get(update).unwrap().clone())),
                ),
                updates: std::mem::take(&mut state.updates),
                _marker: PhantomData,
            };
            target.insert(udpated_state);
        }
    }

    fn updates_to_entity(&self, type_map: &mut CrdtStore, commands: &mut EntityCommands) {
        type_map
            .take::<CrdtLWWState<T>>()
            .map(|state| commands.insert(CrdtLWWStateComponent(state)));
    }
}
