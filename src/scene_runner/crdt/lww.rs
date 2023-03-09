use std::{cell::RefMut, marker::PhantomData};

use bevy::{
    ecs::system::EntityCommands,
    prelude::*,
    utils::{Entry, HashMap},
};
use deno_core::OpState;

use crate::scene_runner::{SceneContext, SceneCrdtTimestamp, SceneEntityId};

use super::{CrdtInterface, FromProto};

#[derive(Component)]
pub struct CrdtLWWState<T: FromProto> {
    pub timestamps: HashMap<SceneEntityId, SceneCrdtTimestamp>,
    pub values: HashMap<SceneEntityId, Option<T>>,
}

impl<T: FromProto> Default for CrdtLWWState<T> {
    fn default() -> Self {
        Self {
            timestamps: Default::default(),
            values: Default::default(),
        }
    }
}

pub struct CrdtLWWInterface<T: FromProto> {
    _marker: PhantomData<T>,
}

impl<T: FromProto> Default for CrdtLWWInterface<T> {
    fn default() -> Self {
        Self {
            _marker: Default::default(),
        }
    }
}

impl<T: FromProto> CrdtInterface for CrdtLWWInterface<T> {
    fn update_crdt(
        &self,
        op_state: &mut RefMut<OpState>,
        entity: SceneEntityId,
        timestamp: SceneCrdtTimestamp,
        data: Option<&mut protobuf::CodedInputStream>,
    ) -> Result<bool, protobuf::Error> {
        // create state if required
        let state = match op_state.try_borrow_mut::<CrdtLWWState<T>>() {
            Some(state) => state,
            None => {
                op_state.put(CrdtLWWState::<T>::default());
                op_state.borrow_mut()
            }
        };

        match state.timestamps.entry(entity) {
            Entry::Occupied(o) => match o.into_mut() {
                last_timestamp if *last_timestamp < timestamp => {
                    state
                        .values
                        .insert(entity, data.map(T::from_proto).transpose()?);
                    *last_timestamp = timestamp;
                    Ok(true)
                }
                _ => Ok(false),
            },
            Entry::Vacant(v) => {
                state
                    .values
                    .insert(entity, data.map(T::from_proto).transpose()?);
                v.insert(timestamp);
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
pub(crate) fn process_crdt_lww_updates<T: FromProto + Component + std::fmt::Debug>(
    mut commands: Commands,
    mut scenes: Query<(Entity, &SceneContext, &mut CrdtLWWState<T>)>,
) {
    for (_root, entity_map, mut updates) in scenes.iter_mut() {
        // remove crdt state for dead entities
        updates
            .timestamps
            .retain(|ent, _| !entity_map.is_dead(*ent));

        for (scene_entity, value) in updates.values.drain() {
            debug!(
                "[{:?}] {} -> {:?}",
                scene_entity,
                std::any::type_name::<T>(),
                value
            );
            let Some(entity) = entity_map.bevy_entity(scene_entity) else {
                info!("skipping {} update for missing entity {:?}", std::any::type_name::<T>(), scene_entity);
                continue;
            };
            match value {
                Some(v) => commands.entity(entity).insert(v),
                None => commands.entity(entity).remove::<T>(),
            };
        }
    }
}
