use bevy::utils::HashMap;
use dcl_component::{DclReader, SceneEntityId};
use std::collections::VecDeque;

const SET_SIZE: usize = 100;

#[derive(Debug, Clone, Hash)]
pub struct CrdtGOEntry {
    // timestamp: SceneCrdtTimestamp,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
pub struct CrdtGOState(pub HashMap<SceneEntityId, VecDeque<CrdtGOEntry>>);

impl CrdtGOState {
    pub fn append(
        &mut self,
        entity: SceneEntityId,
        // timestamp: SceneCrdtTimestamp,
        new_data: &mut DclReader,
    ) {
        let queue = self.0.entry(entity).or_default();
        let new_slot = if queue.len() == SET_SIZE {
            let mut slot = queue.pop_front().unwrap();
            slot.data.clear();
            slot.data.extend_from_slice(new_data.as_slice());
            // slot.timestamp = timestamp;
            slot
        } else {
            CrdtGOEntry {
                // timestamp,
                data: new_data.as_slice().to_vec(),
            }
        };
        queue.push_back(new_slot);
    }
}
