use std::{cell::RefMut, cmp::Ordering, marker::PhantomData};

use bevy::{
    ecs::system::EntityCommands,
    prelude::*,
    utils::{Entry, HashMap},
};
use deno_core::OpState;

use crate::{
    dcl_component::{DclReader, DclReaderError, FromDclReader, SceneCrdtTimestamp, SceneEntityId},
    scene_runner::{DeletedSceneEntities, SceneContext},
};

use super::CrdtInterface;

#[derive(Debug)]
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
                            // update iff data is some
                            maybe_new_data.is_some()
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
                                        Ordering::Less => true,
                                        Ordering::Equal => false,
                                        Ordering::Greater => false,
                                    }
                                }
                            }
                        }
                    }
                };

                if update {
                    // ensure data is valid
                    if maybe_new_data
                        .as_ref()
                        .map(|data| data.len() == 0)
                        .unwrap_or(false)
                    {
                        return Ok(false);
                    }

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
    mut scenes: Query<(
        Entity,
        &SceneContext,
        &mut CrdtLWWState<T>,
        Option<&DeletedSceneEntities>,
    )>,
) {
    for (_root, scene_context, mut updates, maybe_deleted) in scenes.iter_mut() {
        if let Some(deleted_entities) = maybe_deleted {
            // remove crdt state for dead entities
            for deleted in &deleted_entities.0 {
                updates.last_write.remove(deleted);
            }
        }

        for (scene_entity, entry) in updates
            .last_write
            .iter_mut()
            .filter(|(_, entry)| entry.updated)
        {
            entry.updated = false;
            let Some(entity) = scene_context.bevy_entity(*scene_entity) else {
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

#[cfg(test)]
mod test {
    use std::cell::RefCell;

    use super::*;

    impl FromDclReader for u32 {
        fn from_reader(buf: &mut DclReader) -> Result<Self, DclReaderError> {
            Ok(buf.read_u32()?)
        }
    }

    fn assert_entry_eq<T: FromDclReader + Eq + std::fmt::Debug>(
        op_state: RefCell<OpState>,
        entity: SceneEntityId,
        timestamp: SceneCrdtTimestamp,
        data: Option<T>,
    ) {
        let state = op_state.borrow();
        let state = state.borrow::<CrdtLWWState<T>>();
        let Some(LWWEntry {
            timestamp: output_timestamp,
            is_some,
            data: output_data,
            ..
        }) = state.last_write.get(&entity) else {
            panic!("expected entry")
        };

        assert_eq!(*output_timestamp, timestamp);
        assert_eq!(*is_some, data.is_some());

        if let Some(data) = data {
            assert_eq!(
                T::from_reader(&mut DclReader::new(&output_data)).unwrap(),
                data
            );
        }
    }

    #[test]
    fn put_to_none_should_accept() {
        let op_state = RefCell::new(OpState::new(0));
        let interface = CrdtLWWInterface::<u32>::default();

        let entity = SceneEntityId {
            id: 0,
            generation: 0,
        };
        let timestamp = SceneCrdtTimestamp(0);
        let data = 1231u32;
        let buf = 1231u32.to_be_bytes();
        let mut reader = DclReader::new(&buf);

        assert_eq!(
            interface
                .update_crdt(
                    &mut op_state.borrow_mut(),
                    entity,
                    timestamp,
                    Some(&mut reader)
                )
                .unwrap(),
            true
        );

        assert_entry_eq(op_state, entity, timestamp, Some(data));
    }

    #[test]
    fn put_twice_is_idempotent() {
        let op_state = RefCell::new(OpState::new(0));
        let interface = CrdtLWWInterface::<u32>::default();

        let entity = SceneEntityId {
            id: 0,
            generation: 0,
        };
        let timestamp = SceneCrdtTimestamp(0);
        let data = 1231u32;
        let buf = 1231u32.to_be_bytes();

        let mut reader = DclReader::new(&buf);
        assert_eq!(
            interface
                .update_crdt(
                    &mut op_state.borrow_mut(),
                    entity,
                    timestamp,
                    Some(&mut reader)
                )
                .unwrap(),
            true
        );
        let mut reader = DclReader::new(&buf);
        assert_eq!(
            interface
                .update_crdt(
                    &mut op_state.borrow_mut(),
                    entity,
                    timestamp,
                    Some(&mut reader)
                )
                .unwrap(),
            false
        );

        assert_entry_eq(op_state, entity, timestamp, Some(data));
    }

    #[test]
    fn put_newer_should_accept() {
        let op_state = RefCell::new(OpState::new(0));
        let interface = CrdtLWWInterface::<u32>::default();

        let entity = SceneEntityId {
            id: 0,
            generation: 0,
        };
        let timestamp = SceneCrdtTimestamp(0);
        let data = 1231u32;
        let buf = data.to_be_bytes();

        let mut reader = DclReader::new(&buf);
        assert_eq!(
            interface
                .update_crdt(
                    &mut op_state.borrow_mut(),
                    entity,
                    timestamp,
                    Some(&mut reader)
                )
                .unwrap(),
            true
        );

        let timestamp = SceneCrdtTimestamp(1);
        let newer_data = 999u32;
        let buf = newer_data.to_be_bytes();
        let mut reader = DclReader::new(&buf);
        assert_eq!(
            interface
                .update_crdt(
                    &mut op_state.borrow_mut(),
                    entity,
                    timestamp,
                    Some(&mut reader)
                )
                .unwrap(),
            true
        );

        assert_entry_eq(op_state, entity, timestamp, Some(newer_data));
    }

    #[test]
    fn put_older_should_fail() {
        let op_state = RefCell::new(OpState::new(0));
        let interface = CrdtLWWInterface::<u32>::default();

        let entity = SceneEntityId {
            id: 0,
            generation: 0,
        };
        let timestamp = SceneCrdtTimestamp(1);
        let data = 1231u32;
        let buf = data.to_be_bytes();

        let mut reader = DclReader::new(&buf);
        assert_eq!(
            interface
                .update_crdt(
                    &mut op_state.borrow_mut(),
                    entity,
                    timestamp,
                    Some(&mut reader)
                )
                .unwrap(),
            true
        );

        let older_timestamp = SceneCrdtTimestamp(0);
        let newer_data = 999u32;
        let buf = newer_data.to_be_bytes();
        let mut reader = DclReader::new(&buf);
        assert_eq!(
            interface
                .update_crdt(
                    &mut op_state.borrow_mut(),
                    entity,
                    older_timestamp,
                    Some(&mut reader)
                )
                .unwrap(),
            false
        );

        assert_entry_eq(op_state, entity, timestamp, Some(data));
    }

    #[test]
    fn put_higher_value_should_accept() {
        let op_state = RefCell::new(OpState::new(0));
        let interface = CrdtLWWInterface::<u32>::default();

        let entity = SceneEntityId {
            id: 0,
            generation: 0,
        };
        let timestamp = SceneCrdtTimestamp(1);
        let data = 1u32;
        let buf = data.to_be_bytes();

        let mut reader = DclReader::new(&buf);
        assert_eq!(
            interface
                .update_crdt(
                    &mut op_state.borrow_mut(),
                    entity,
                    timestamp,
                    Some(&mut reader)
                )
                .unwrap(),
            true
        );

        let higher_data = 2u32;
        let buf = higher_data.to_be_bytes();
        let mut reader = DclReader::new(&buf);
        assert_eq!(
            interface
                .update_crdt(
                    &mut op_state.borrow_mut(),
                    entity,
                    timestamp,
                    Some(&mut reader)
                )
                .unwrap(),
            true
        );

        assert_entry_eq(op_state, entity, timestamp, Some(higher_data));
    }

    #[test]
    fn delete_same_timestamp_should_reject() {
        let op_state = RefCell::new(OpState::new(0));
        let interface = CrdtLWWInterface::<u32>::default();

        let entity = SceneEntityId {
            id: 0,
            generation: 0,
        };
        let timestamp = SceneCrdtTimestamp(1);
        let data = 1u32;
        let buf = data.to_be_bytes();

        let mut reader = DclReader::new(&buf);
        assert_eq!(
            interface
                .update_crdt(
                    &mut op_state.borrow_mut(),
                    entity,
                    timestamp,
                    Some(&mut reader)
                )
                .unwrap(),
            true
        );

        assert_eq!(
            interface
                .update_crdt(&mut op_state.borrow_mut(), entity, timestamp, None)
                .unwrap(),
            false
        );

        assert_entry_eq(op_state, entity, timestamp, Some(data));
    }

    #[test]
    fn delete_newer_should_accept() {
        let op_state = RefCell::new(OpState::new(0));
        let interface = CrdtLWWInterface::<u32>::default();

        let entity = SceneEntityId {
            id: 0,
            generation: 0,
        };
        let timestamp = SceneCrdtTimestamp(1);
        let data = 1u32;
        let buf = data.to_be_bytes();

        let mut reader = DclReader::new(&buf);
        assert_eq!(
            interface
                .update_crdt(
                    &mut op_state.borrow_mut(),
                    entity,
                    timestamp,
                    Some(&mut reader)
                )
                .unwrap(),
            true
        );

        let newer_timestamp = SceneCrdtTimestamp(2);
        assert_eq!(
            interface
                .update_crdt(&mut op_state.borrow_mut(), entity, newer_timestamp, None)
                .unwrap(),
            true
        );

        assert_entry_eq(op_state, entity, newer_timestamp, Option::<u32>::None);
    }

    #[test]
    fn delete_is_idempotent() {
        let op_state = RefCell::new(OpState::new(0));
        let interface = CrdtLWWInterface::<u32>::default();

        let entity = SceneEntityId {
            id: 0,
            generation: 0,
        };
        let timestamp = SceneCrdtTimestamp(1);
        let data = 1u32;
        let buf = data.to_be_bytes();

        let mut reader = DclReader::new(&buf);
        assert_eq!(
            interface
                .update_crdt(
                    &mut op_state.borrow_mut(),
                    entity,
                    timestamp,
                    Some(&mut reader)
                )
                .unwrap(),
            true
        );

        let newer_timestamp = SceneCrdtTimestamp(2);
        assert_eq!(
            interface
                .update_crdt(&mut op_state.borrow_mut(), entity, newer_timestamp, None)
                .unwrap(),
            true
        );
        assert_eq!(
            interface
                .update_crdt(&mut op_state.borrow_mut(), entity, newer_timestamp, None)
                .unwrap(),
            false
        );

        assert_entry_eq(op_state, entity, newer_timestamp, Option::<u32>::None);
    }

    #[test]
    fn put_with_delete_timestamp_should_accept() {
        let op_state = RefCell::new(OpState::new(0));
        let interface = CrdtLWWInterface::<u32>::default();

        let entity = SceneEntityId {
            id: 0,
            generation: 0,
        };
        let timestamp = SceneCrdtTimestamp(1);
        let data = 1u32;
        let buf = data.to_be_bytes();

        let mut reader = DclReader::new(&buf);
        assert_eq!(
            interface
                .update_crdt(
                    &mut op_state.borrow_mut(),
                    entity,
                    timestamp,
                    Some(&mut reader)
                )
                .unwrap(),
            true
        );

        let newer_timestamp = SceneCrdtTimestamp(2);
        assert_eq!(
            interface
                .update_crdt(&mut op_state.borrow_mut(), entity, newer_timestamp, None)
                .unwrap(),
            true
        );

        let data = 3u32;
        let buf = data.to_be_bytes();
        let mut reader = DclReader::new(&buf);
        assert_eq!(
            interface
                .update_crdt(
                    &mut op_state.borrow_mut(),
                    entity,
                    newer_timestamp,
                    Some(&mut reader)
                )
                .unwrap(),
            true
        );

        assert_entry_eq(op_state, entity, newer_timestamp, Some(data));
    }

    #[test]
    fn put_rejects_null_data() {
        let op_state = RefCell::new(OpState::new(0));
        let interface = CrdtLWWInterface::<u32>::default();

        let entity = SceneEntityId {
            id: 0,
            generation: 0,
        };
        let timestamp = SceneCrdtTimestamp(0);
        let data = 1231u32;
        let buf = 1231u32.to_be_bytes();

        let mut reader = DclReader::new(&buf);
        assert_eq!(
            interface
                .update_crdt(
                    &mut op_state.borrow_mut(),
                    entity,
                    timestamp,
                    Some(&mut reader)
                )
                .unwrap(),
            true
        );

        let newer_timestamp = SceneCrdtTimestamp(2);
        let mut reader = DclReader::new(&[]);
        assert_eq!(
            interface
                .update_crdt(
                    &mut op_state.borrow_mut(),
                    entity,
                    newer_timestamp,
                    Some(&mut reader)
                )
                .unwrap(),
            false
        );

        assert_entry_eq(op_state, entity, timestamp, Some(data));
    }
}
