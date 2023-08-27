use std::cmp::Ordering;

use bevy::utils::{Entry, HashMap, HashSet};

use dcl_component::{DclReader, SceneCrdtTimestamp, SceneEntityId};

#[derive(Debug, Clone)]
pub struct LWWEntry {
    pub timestamp: SceneCrdtTimestamp,
    pub is_some: bool,
    pub data: Vec<u8>,
}

#[derive(Clone, Default, Debug)]
pub struct CrdtLWWState {
    pub last_write: HashMap<SceneEntityId, LWWEntry>,
    pub updates: HashSet<SceneEntityId>,
}

enum UpdateMode {
    Force,
    ForceIfDifferent,
    Normal,
}

impl CrdtLWWState {
    fn check_update(
        entry: &LWWEntry,
        new_timestamp: SceneCrdtTimestamp,
        maybe_new_data: Option<&DclReader>,
    ) -> bool {
        match entry.timestamp.cmp(&new_timestamp) {
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
        }
    }

    fn perform_update(
        &mut self,
        entity: SceneEntityId,
        new_timestamp: SceneCrdtTimestamp,
        maybe_new_data: Option<&mut DclReader>,
        mode: UpdateMode,
    ) -> Option<SceneCrdtTimestamp> {
        match self.last_write.entry(entity) {
            Entry::Occupied(o) => {
                let entry = o.into_mut();
                let update = match mode {
                    UpdateMode::Force => true,
                    UpdateMode::ForceIfDifferent => {
                        entry.is_some != maybe_new_data.is_some()
                            || entry.data.as_slice()
                                != maybe_new_data
                                    .as_ref()
                                    .map(|r| r.as_slice())
                                    .unwrap_or_default()
                    }
                    UpdateMode::Normal => {
                        Self::check_update(entry, new_timestamp, maybe_new_data.as_deref())
                    }
                };

                if update {
                    entry.timestamp = if !matches!(mode, UpdateMode::Normal) {
                        SceneCrdtTimestamp(entry.timestamp.0 + 1)
                    } else {
                        new_timestamp
                    };

                    entry.data.clear();
                    match maybe_new_data {
                        Some(new_data) => {
                            entry.is_some = true;
                            entry.data.extend_from_slice(new_data.as_slice());
                        }
                        None => entry.is_some = false,
                    }
                    self.updates.insert(entity);
                    Some(entry.timestamp)
                } else {
                    None
                }
            }
            Entry::Vacant(v) => {
                v.insert(LWWEntry {
                    timestamp: new_timestamp,
                    is_some: maybe_new_data.is_some(),
                    data: maybe_new_data
                        .map(|new_data| new_data.as_slice().to_vec())
                        .unwrap_or_default(),
                });
                self.updates.insert(entity);
                Some(new_timestamp)
            }
        }
    }

    pub fn try_update(
        &mut self,
        entity: SceneEntityId,
        new_timestamp: SceneCrdtTimestamp,
        maybe_new_data: Option<&mut DclReader>,
    ) -> bool {
        self.perform_update(entity, new_timestamp, maybe_new_data, UpdateMode::Normal)
            .is_some()
    }

    pub fn force_update(
        &mut self,
        entity: SceneEntityId,
        maybe_new_data: Option<&mut DclReader>,
    ) -> SceneCrdtTimestamp {
        self.perform_update(
            entity,
            SceneCrdtTimestamp(0),
            maybe_new_data,
            UpdateMode::Force,
        )
        .unwrap()
    }

    pub fn update_if_different(
        &mut self,
        entity: SceneEntityId,
        maybe_new_data: Option<&mut DclReader>,
    ) -> Option<SceneCrdtTimestamp> {
        self.perform_update(
            entity,
            SceneCrdtTimestamp(0),
            maybe_new_data,
            UpdateMode::ForceIfDifferent,
        )
    }
}

#[cfg(test)]
mod test {
    use dcl_component::FromDclReader;

    use super::*;

    fn assert_entry_eq<T: FromDclReader + Eq + std::fmt::Debug>(
        state: CrdtLWWState,
        entity: SceneEntityId,
        timestamp: SceneCrdtTimestamp,
        data: Option<T>,
    ) {
        let Some(LWWEntry {
            timestamp: output_timestamp,
            is_some,
            data: output_data,
            ..
        }) = state.last_write.get(&entity)
        else {
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
        let mut state = CrdtLWWState::default();
        let entity = SceneEntityId {
            id: 0,
            generation: 0,
        };
        let timestamp = SceneCrdtTimestamp(0);
        let data = 1231u32;
        let buf = data.to_le_bytes();
        let mut reader = DclReader::new(&buf);

        assert_eq!(state.try_update(entity, timestamp, Some(&mut reader)), true);

        assert_entry_eq(state, entity, timestamp, Some(data));
    }

    #[test]
    fn put_twice_is_idempotent() {
        let mut state = CrdtLWWState::default();

        let entity = SceneEntityId {
            id: 0,
            generation: 0,
        };
        let timestamp = SceneCrdtTimestamp(0);
        let data = 1231u32;
        let buf = data.to_le_bytes();

        let mut reader = DclReader::new(&buf);
        assert_eq!(state.try_update(entity, timestamp, Some(&mut reader)), true);
        let mut reader = DclReader::new(&buf);
        assert_eq!(
            state.try_update(entity, timestamp, Some(&mut reader)),
            false
        );

        assert_entry_eq(state, entity, timestamp, Some(data));
    }

    #[test]
    fn put_newer_should_accept() {
        let mut state = CrdtLWWState::default();

        let entity = SceneEntityId {
            id: 0,
            generation: 0,
        };
        let timestamp = SceneCrdtTimestamp(0);
        let data = 1231u32;
        let buf = data.to_le_bytes();

        let mut reader = DclReader::new(&buf);
        assert_eq!(state.try_update(entity, timestamp, Some(&mut reader)), true);

        let timestamp = SceneCrdtTimestamp(1);
        let newer_data = 999u32;
        let buf = newer_data.to_le_bytes();
        let mut reader = DclReader::new(&buf);
        assert_eq!(state.try_update(entity, timestamp, Some(&mut reader)), true);

        assert_entry_eq(state, entity, timestamp, Some(newer_data));
    }

    #[test]
    fn put_older_should_fail() {
        let mut state = CrdtLWWState::default();

        let entity = SceneEntityId {
            id: 0,
            generation: 0,
        };
        let timestamp = SceneCrdtTimestamp(1);
        let data = 1231u32;
        let buf = data.to_le_bytes();

        let mut reader = DclReader::new(&buf);
        assert_eq!(state.try_update(entity, timestamp, Some(&mut reader)), true);

        let older_timestamp = SceneCrdtTimestamp(0);
        let newer_data = 999u32;
        let buf = newer_data.to_le_bytes();
        let mut reader = DclReader::new(&buf);
        assert_eq!(
            state.try_update(entity, older_timestamp, Some(&mut reader)),
            false
        );

        assert_entry_eq(state, entity, timestamp, Some(data));
    }

    #[test]
    fn put_higher_value_should_accept() {
        let mut state = CrdtLWWState::default();

        let entity = SceneEntityId {
            id: 0,
            generation: 0,
        };
        let timestamp = SceneCrdtTimestamp(1);
        let data = 1u32;
        let buf = data.to_le_bytes();

        let mut reader = DclReader::new(&buf);
        assert_eq!(state.try_update(entity, timestamp, Some(&mut reader)), true);

        let higher_data = 2u32;
        let buf = higher_data.to_le_bytes();
        let mut reader = DclReader::new(&buf);
        assert_eq!(state.try_update(entity, timestamp, Some(&mut reader)), true);

        assert_entry_eq(state, entity, timestamp, Some(higher_data));
    }

    #[test]
    fn delete_same_timestamp_should_reject() {
        let mut state = CrdtLWWState::default();

        let entity = SceneEntityId {
            id: 0,
            generation: 0,
        };
        let timestamp = SceneCrdtTimestamp(1);
        let data = 1u32;
        let buf = data.to_le_bytes();

        let mut reader = DclReader::new(&buf);
        assert_eq!(state.try_update(entity, timestamp, Some(&mut reader)), true);

        assert_eq!(state.try_update(entity, timestamp, None), false);

        assert_entry_eq(state, entity, timestamp, Some(data));
    }

    #[test]
    fn delete_newer_should_accept() {
        let mut state = CrdtLWWState::default();

        let entity = SceneEntityId {
            id: 0,
            generation: 0,
        };
        let timestamp = SceneCrdtTimestamp(1);
        let data = 1u32;
        let buf = data.to_le_bytes();

        let mut reader = DclReader::new(&buf);
        assert_eq!(state.try_update(entity, timestamp, Some(&mut reader)), true);

        let newer_timestamp = SceneCrdtTimestamp(2);
        assert_eq!(state.try_update(entity, newer_timestamp, None), true);

        assert_entry_eq(state, entity, newer_timestamp, Option::<u32>::None);
    }

    #[test]
    fn delete_is_idempotent() {
        let mut state = CrdtLWWState::default();

        let entity = SceneEntityId {
            id: 0,
            generation: 0,
        };
        let timestamp = SceneCrdtTimestamp(1);
        let data = 1u32;
        let buf = data.to_le_bytes();

        let mut reader = DclReader::new(&buf);
        assert_eq!(state.try_update(entity, timestamp, Some(&mut reader)), true);

        let newer_timestamp = SceneCrdtTimestamp(2);
        assert_eq!(state.try_update(entity, newer_timestamp, None), true);
        assert_eq!(state.try_update(entity, newer_timestamp, None), false);

        assert_entry_eq(state, entity, newer_timestamp, Option::<u32>::None);
    }

    #[test]
    fn put_with_delete_timestamp_should_accept() {
        let mut state = CrdtLWWState::default();

        let entity = SceneEntityId {
            id: 0,
            generation: 0,
        };
        let timestamp = SceneCrdtTimestamp(1);
        let data = 1u32;
        let buf = data.to_le_bytes();

        let mut reader = DclReader::new(&buf);
        assert_eq!(state.try_update(entity, timestamp, Some(&mut reader)), true);

        let newer_timestamp = SceneCrdtTimestamp(2);
        assert_eq!(state.try_update(entity, newer_timestamp, None), true);

        let data = 3u32;
        let buf = data.to_le_bytes();
        let mut reader = DclReader::new(&buf);
        assert_eq!(
            state.try_update(entity, newer_timestamp, Some(&mut reader)),
            true
        );

        assert_entry_eq(state, entity, newer_timestamp, Some(data));
    }

    #[test]
    fn put_accepts_null_data() {
        let mut state = CrdtLWWState::default();

        let entity = SceneEntityId {
            id: 0,
            generation: 0,
        };
        let timestamp = SceneCrdtTimestamp(0);
        let buf = 1231u32.to_le_bytes();

        let mut reader = DclReader::new(&buf);
        assert_eq!(state.try_update(entity, timestamp, Some(&mut reader)), true);

        let newer_timestamp = SceneCrdtTimestamp(2);
        let mut reader = DclReader::new(&[]);
        assert_eq!(
            state.try_update(entity, newer_timestamp, Some(&mut reader)),
            true
        );

        assert_entry_eq(state, entity, newer_timestamp, Some(Vec::<u8>::default()));
    }
}
