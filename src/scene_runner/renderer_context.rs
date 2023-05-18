use bevy::{prelude::*, utils::HashSet};

use crate::{
    dcl::{
        interface::{CrdtStore, CrdtType},
        SceneId,
    },
    dcl_assert,
    dcl_component::{DclReader, DclWriter, SceneComponentId, SceneEntityId, ToDclWriter},
};

// contains a list of (SceneEntityId.generation, bevy entity) indexed by SceneEntityId.id
// where generation is the earliest non-dead (though maybe not yet live)
// generation for the scene id index.
// entities are initialized within the engine message-loop op, and added to 'nascent' until
// the process_lifecycle system enlivens them.
// Bevy entities are only created on a PUT of a component we care about in the renderer,
// or if they are required for hierarchy parenting
// TODO - consider Vec<Option<page>>
type LiveEntityTable = Vec<(u16, Option<Entity>)>;

// mapping from script entity -> bevy entity
// note - be careful with size as this struct is moved into/out of js runtimes
#[derive(Component, Debug)]
pub struct RendererSceneContext {
    pub scene_id: SceneId,
    pub base: IVec2,
    pub priority: f32,

    // entities waiting to be born in bevy
    pub nascent: HashSet<SceneEntityId>,
    // entities waiting to be destroyed in bevy
    pub death_row: HashSet<SceneEntityId>,
    // entities that are live
    live_entities: LiveEntityTable,

    // list of entities that are not currently parented to their target parent
    pub unparented_entities: HashSet<Entity>,
    // indicates if we need to reprocess unparented entities
    pub hierarchy_changed: bool,

    // time of last message sent to scene
    pub last_sent: f32,
    // time of last updates to bevy world from scene
    pub last_update_frame: u32,
    // currently running?
    pub in_flight: bool,
    // currently broken (record and keep for debug purposes and to avoid spamming reloads)
    pub broken: bool,

    pub crdt_store: CrdtStore,

    // readiness to update, if anything blocks the scene should not run
    pub blocked: HashSet<&'static str>,

    // total scene run time in seconds
    pub total_runtime: f32,
    // scene tick number
    pub tick_number: u32,
}

impl RendererSceneContext {
    pub fn new(scene_id: SceneId, base: IVec2, root: Entity, priority: f32) -> Self {
        let mut new_context = Self {
            scene_id,
            base,
            nascent: Default::default(),
            death_row: Default::default(),
            live_entities: Vec::from_iter(std::iter::repeat((0, None)).take(u16::MAX as usize)),
            unparented_entities: HashSet::new(),
            hierarchy_changed: false,
            last_sent: 0.0,
            last_update_frame: 0,
            in_flight: false,
            broken: false,
            priority,
            crdt_store: Default::default(),
            blocked: Default::default(),
            total_runtime: 0.0,
            tick_number: u32::MAX,
        };

        new_context.live_entities[SceneEntityId::ROOT.id as usize] =
            (SceneEntityId::ROOT.generation, Some(root));
        new_context
    }

    fn entity_entry(&self, id: u16) -> &(u16, Option<Entity>) {
        // SAFETY: live entities has u16::MAX members
        unsafe { self.live_entities.get_unchecked(id as usize) }
    }

    fn entity_entry_mut(&mut self, id: u16) -> &mut (u16, Option<Entity>) {
        // SAFETY: live entities has u16::MAX members
        unsafe { self.live_entities.get_unchecked_mut(id as usize) }
    }

    pub fn associate_bevy_entity(&mut self, scene_entity: SceneEntityId, bevy_entity: Entity) {
        debug!(
            "[{:?}] associate scene id: {} -> bevy id {:?}",
            self.scene_id, scene_entity, bevy_entity
        );
        dcl_assert!(self.entity_entry(scene_entity.id).0 <= scene_entity.generation);
        dcl_assert!(self.entity_entry(scene_entity.id).1.is_none());
        *self.entity_entry_mut(scene_entity.id) = (scene_entity.generation, Some(bevy_entity));
    }

    pub fn bevy_entity(&self, scene_entity: SceneEntityId) -> Option<Entity> {
        match self.entity_entry(scene_entity.id) {
            (gen, Some(bevy_entity)) if *gen == scene_entity.generation => Some(*bevy_entity),
            _ => None,
        }
    }

    pub fn set_dead(&mut self, entity: SceneEntityId) {
        let entry = self.entity_entry_mut(entity.id);
        if entry.0 == entity.generation {
            entry.0 += 1;
            entry.1 = None;
        }
    }

    pub fn is_dead(&self, entity: SceneEntityId) -> bool {
        self.entity_entry(entity.id).0 > entity.generation
    }

    pub fn update_crdt(
        &mut self,
        component_id: SceneComponentId,
        crdt_type: CrdtType,
        id: SceneEntityId,
        data: &impl ToDclWriter,
    ) {
        let mut buf = Vec::new();
        DclWriter::new(&mut buf).write(data);
        self.crdt_store
            .force_update(component_id, crdt_type, id, Some(&mut DclReader::new(&buf)));
    }

    #[allow(dead_code)]
    pub fn clear_crdt(
        &mut self,
        component_id: SceneComponentId,
        crdt_type: CrdtType,
        id: SceneEntityId,
    ) {
        self.crdt_store
            .force_update(component_id, crdt_type, id, None);
    }
}
