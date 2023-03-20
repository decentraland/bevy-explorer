// interface managing data transfer for LWW components

use bevy::utils::HashMap;

use crate::{
    dcl::crdt::lww::CrdtLWWState,
    dcl_component::{
        DclReader, DclReaderError, SceneComponentId, SceneCrdtTimestamp, SceneEntityId,
    },
};

use super::CrdtStore;

pub fn update_crdt(
    target: &mut CrdtStore,
    component_id: SceneComponentId,
    entity: SceneEntityId,
    new_timestamp: SceneCrdtTimestamp,
    maybe_new_data: Option<&mut DclReader>,
) -> Result<bool, DclReaderError> {
    // create state if required
    let state = target
        .lww
        .entry(component_id)
        .or_insert_with(CrdtLWWState::default);
    state.update(entity, new_timestamp, maybe_new_data)
}

pub fn take_updates(
    component_id: SceneComponentId,
    source: &mut CrdtStore,
    target: &mut CrdtStore,
) {
    if let Some(state) = source.lww.get_mut(&component_id) {
        let udpated_state = CrdtLWWState {
            last_write: HashMap::from_iter(
                state
                    .updates
                    .iter()
                    .map(|update| (*update, state.last_write.get(update).unwrap().clone())),
            ),
            updates: std::mem::take(&mut state.updates),
        };
        target.lww.insert(component_id, udpated_state);
    }
}
