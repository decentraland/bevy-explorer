use std::ops::RangeInclusive;

use bevy::{prelude::debug, platform::collections::HashSet};

use crate::{SceneCensus, SceneId};
use dcl_component::SceneEntityId;

type LiveTable = Vec<(u16, bool)>;

pub struct CrdtContext {
    pub scene_id: SceneId,
    pub hash: String,
    pub testing: bool,
    pub preview: bool,
    live_entities: LiveTable,
    nascent: HashSet<SceneEntityId>,
    death_row: HashSet<SceneEntityId>,
    last_new: u16,
}

impl CrdtContext {
    pub fn new(scene_id: SceneId, hash: String, testing: bool, preview: bool) -> Self {
        Self {
            scene_id,
            hash,
            testing,
            preview,
            live_entities: vec![(0, false); u16::MAX as usize],
            nascent: Default::default(),
            death_row: Default::default(),
            last_new: u16::MAX,
        }
    }

    fn entity_entry(&self, id: u16) -> &(u16, bool) {
        // SAFETY: live entities has u16::MAX members
        unsafe { self.live_entities.get_unchecked(id as usize) }
    }

    // queue an entity for creation if required
    // returns false if the entity is already dead
    pub fn init(&mut self, entity: SceneEntityId) -> bool {
        // debug!(" init {:?}!", entity);
        if self.is_dead(entity) {
            debug!("{:?} is dead!", entity);
            return false;
        }

        if !self.is_born(entity) {
            debug!("scene added {entity:?}");
            self.nascent.insert(entity);
        } else {
            // debug!("{:?} is live already!", entity);
        }

        true
    }

    pub fn take_census(&mut self) -> SceneCensus {
        for scene_entity in &self.nascent {
            self.live_entities[scene_entity.id as usize] = (scene_entity.generation, true);
        }

        SceneCensus {
            scene_id: self.scene_id,
            born: std::mem::take(&mut self.nascent),
            died: std::mem::take(&mut self.death_row),
        }
    }

    pub fn kill(&mut self, scene_entity: SceneEntityId) {
        // update entity table and death row
        match &mut self.live_entities[scene_entity.id as usize] {
            (gen, live) if *gen <= scene_entity.generation => {
                *gen = scene_entity.generation + 1;

                if *live {
                    self.death_row.insert(scene_entity);
                }
                *live = false;
            }
            _ => (),
        }

        // remove from nascent
        self.nascent.remove(&scene_entity);
        debug!("scene killed {scene_entity:?}");
    }

    pub fn is_born(&self, scene_entity: SceneEntityId) -> bool {
        self.nascent.contains(&scene_entity) || {
            let entry = self.entity_entry(scene_entity.id);
            entry.0 == scene_entity.generation && entry.1
        }
    }

    pub fn is_dead(&self, entity: SceneEntityId) -> bool {
        self.entity_entry(entity.id).0 > entity.generation
    }

    pub fn new_in_range(&mut self, range: &RangeInclusive<u16>) -> Option<SceneEntityId> {
        let mut next_new = self.last_new.wrapping_add(1);
        if !range.contains(&self.last_new) {
            self.last_new = *range.end();
            next_new = *range.start();
        }

        while next_new != self.last_new {
            if !self.entity_entry(next_new).1 {
                let new_id = SceneEntityId::new(next_new, self.entity_entry(next_new).0);
                self.init(new_id);
                self.last_new = next_new;
                return Some(new_id);
            }
            next_new += 1;
            if !range.contains(&self.last_new) {
                self.last_new = *range.start();
            }
        }

        None
    }
}
