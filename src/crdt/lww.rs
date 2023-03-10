use std::{cell::RefMut, cmp::Ordering, marker::PhantomData};

use bevy::{
    ecs::system::EntityCommands,
    prelude::*,
    utils::{Entry, HashMap},
};
use deno_core::OpState;

use crate::{
    dcl_component::{DclReader, DclReaderError, FromDclReader, SceneCrdtTimestamp, SceneEntityId},
    scene_runner::SceneContext,
};

use super::CrdtInterface;

pub struct LWWEntry {
    pub timestamp: SceneCrdtTimestamp,
    pub updated: bool,
    pub is_some: bool,
    pub data: Vec<u8>,
}

#[derive(Component)]
pub struct CrdtLWWState<T> {
    pub last_write: HashMap<SceneEntityId, LWWEntry>,
    _marker: PhantomData<T>,
}

impl<T: FromDclReader> Default for CrdtLWWState<T> {
    fn default() -> Self {
        Self {
            last_write: Default::default(),
            _marker: PhantomData,
        }
    }
}

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
        op_state: &mut RefMut<OpState>,
        entity: SceneEntityId,
        new_timestamp: SceneCrdtTimestamp,
        maybe_new_data: Option<&mut DclReader>,
    ) -> Result<bool, DclReaderError> {
        // create state if required
        let state = match op_state.try_borrow_mut::<CrdtLWWState<T>>() {
            Some(state) => state,
            None => {
                op_state.put(CrdtLWWState::<T>::default());
                op_state.borrow_mut()
            }
        };

        match state.last_write.entry(entity) {
            Entry::Occupied(o) => {
                let entry = o.into_mut();
                let update = match entry.timestamp.cmp(&new_timestamp) {
                    // current is newer
                    Ordering::Greater => false,
                    // current is older
                    Ordering::Less => true,
                    Ordering::Equal => {
                        if !entry.is_some {
                            // timestamps are equal, current is none
                            true
                        } else {
                            let current_len = entry.data.len() + 1;
                            let new_len = match maybe_new_data.as_ref() {
                                Some(new_data) => new_data.len() + 1,
                                None => 0,
                            };
                            match current_len.cmp(&new_len) {
                                // current is longer, don't update
                                Ordering::Greater => false,
                                // current is shorter
                                Ordering::Less => true,
                                Ordering::Equal => {
                                    // compare bytes
                                    match entry
                                        .data
                                        .as_slice()
                                        .cmp(maybe_new_data.as_ref().unwrap().as_slice())
                                    {
                                        Ordering::Less => false,
                                        Ordering::Equal => false,
                                        Ordering::Greater => true,
                                    }
                                }
                            }
                        }
                    }
                };

                if update {
                    entry.timestamp = new_timestamp;
                    entry.updated = true;

                    entry.data.clear();
                    match maybe_new_data {
                        Some(new_data) => {
                            entry.is_some = true;
                            entry.data.extend_from_slice(new_data.as_slice());
                        }
                        None => entry.is_some = false,
                    }
                }
                Ok(update)
            }
            Entry::Vacant(v) => {
                v.insert(LWWEntry {
                    timestamp: new_timestamp,
                    updated: true,
                    is_some: maybe_new_data.is_some(),
                    data: maybe_new_data
                        .map(|new_data| new_data.as_slice().to_vec())
                        .unwrap_or_default(),
                });
                Ok(true)
            }
        }
    }

    fn claim_crdt(&self, op_state: &mut RefMut<OpState>, commands: &mut EntityCommands) {
        op_state
            .try_take::<CrdtLWWState<T>>()
            .map(|state| commands.insert(state));
    }
}

// a default system for processing LWW comonent updates
pub(crate) fn process_crdt_lww_updates<T: FromDclReader + Component + std::fmt::Debug>(
    mut commands: Commands,
    mut scenes: Query<(Entity, &SceneContext, &mut CrdtLWWState<T>)>,
) {
    for (_root, entity_map, mut updates) in scenes.iter_mut() {
        // remove crdt state for dead entities
        updates
            .last_write
            .retain(|ent, _| !entity_map.is_dead(*ent));

        for (scene_entity, entry) in updates
            .last_write
            .iter_mut()
            .filter(|(_, entry)| entry.updated)
        {
            entry.updated = false;
            let Some(entity) = entity_map.bevy_entity(*scene_entity) else {
                info!("skipping {} update for missing entity {:?}", std::any::type_name::<T>(), scene_entity);
                continue;
            };
            if entry.is_some {
                match T::from_reader(&mut DclReader::new(&entry.data)) {
                    Ok(t) => {
                        debug!(
                            "[{:?}] {} -> {:?}",
                            scene_entity,
                            std::any::type_name::<T>(),
                            t
                        );
                        commands.entity(entity).insert(t);
                    }
                    Err(e) => {
                        warn!(
                            "failed to deserialize {} from buffer: {:?}",
                            std::any::type_name::<T>(),
                            e
                        );
                    }
                };
            } else {
                commands.entity(entity).remove::<T>();
            }
        }
    }
}
