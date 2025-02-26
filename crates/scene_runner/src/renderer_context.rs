use bevy::{prelude::*, utils::HashSet};

use common::{
    structs::SpawnPoint,
    util::{dcl_assert, RingBuffer},
};
use dcl::{
    interface::{CrdtStore, CrdtType},
    SceneId, SceneLogMessage,
};
use dcl_component::{DclReader, DclWriter, SceneComponentId, SceneEntityId, ToDclWriter};
use scene_material::BoundRegion;

use crate::{
    primary_entities::PrimaryEntities,
    update_world::{
        mesh_collider::DisableCollisions,
        transform_and_parent::{ParentPositionSync, SceneProxyStage},
    },
    ContainerEntity, SceneEntity, TargetParent,
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
    pub hash: String,
    pub storage_root: String,
    pub is_portable: bool,
    pub title: String,
    pub base: IVec2,
    pub parcels: HashSet<IVec2>,
    // world-space bounds for the scene
    pub bounds: Vec<BoundRegion>,
    pub spawn_points: Vec<SpawnPoint>,
    pub priority: f32,
    pub size: UVec2,

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
    // last tick delta
    pub last_update_dt: f32,

    // message buffer
    pub logs: RingBuffer<SceneLogMessage>,
    log_to_stdout: bool,

    // last time a pointer event occurred
    pub last_action_event: Option<f32>,
    // sdk version
    pub sdk_version: &'static str,

    // if an inspector is attached
    pub inspected: bool,
}

pub const SCENE_LOG_BUFFER_SIZE: usize = 100;

impl RendererSceneContext {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        scene_id: SceneId,
        hash: String,
        storage_root: String,
        is_portable: bool,
        title: String,
        base: IVec2,
        parcels: HashSet<IVec2>,
        bounds: Vec<BoundRegion>,
        spawn_points: Vec<SpawnPoint>,
        root: Entity,
        size: UVec2,
        priority: f32,
        log_to_stdout: bool,
        sdk_version: &'static str,
        inspected: bool,
    ) -> Self {
        let mut new_context = Self {
            scene_id,
            hash,
            storage_root,
            is_portable,
            title,
            base,
            parcels,
            bounds,
            spawn_points,
            size,
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
            tick_number: 0,
            last_update_dt: 0.0,
            logs: RingBuffer::new(1000, 100),
            log_to_stdout,
            last_action_event: None,
            sdk_version,
            inspected,
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

    pub fn spawn_bevy_entity(
        &mut self,
        commands: &mut Commands,
        root: Entity,
        id: SceneEntityId,
        primaries: &PrimaryEntities,
    ) -> Entity {
        dcl_assert!(self.bevy_entity(id).is_none());

        let spawned = commands
            .spawn((
                SpatialBundle::default(),
                SceneEntity {
                    scene_id: self.scene_id,
                    root,
                    id,
                },
                TargetParent(root),
            ))
            .id();

        commands.entity(spawned).try_insert(ContainerEntity {
            root,
            container: spawned,
            container_id: id,
        });

        if id == SceneEntityId::CAMERA {
            commands.entity(spawned).try_insert((
                ParentPositionSync::<SceneProxyStage>::new(primaries.camera()),
                DisableCollisions,
            ));
        }
        if id == SceneEntityId::PLAYER {
            commands.entity(spawned).try_insert((
                ParentPositionSync::<SceneProxyStage>::new(primaries.player()),
                DisableCollisions,
            ));
        }

        commands.entity(root).add_child(spawned);

        self.associate_bevy_entity(id, spawned);

        self.hierarchy_changed = true;

        debug!(
            "spawned {:?}/{:?} -> {:?}",
            root,
            id,
            self.bevy_entity(id).unwrap()
        );

        spawned
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

    pub fn log(&mut self, log: SceneLogMessage) {
        if self.log_to_stdout {
            match log.level {
                dcl::SceneLogLevel::Log => {
                    info!("[{} {}] {}", self.base, log.timestamp, log.message)
                }
                dcl::SceneLogLevel::SceneError => {
                    warn!("[{} {}] {}", self.base, log.timestamp, log.message)
                }
                dcl::SceneLogLevel::SystemError => {
                    error!("[{} {}] {}", self.base, log.timestamp, log.message)
                }
            }
        }

        self.logs.send(log);
    }
}
