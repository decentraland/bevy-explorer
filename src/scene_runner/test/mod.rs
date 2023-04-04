use std::{path::PathBuf, sync::Mutex};

use std::{collections::BTreeMap, fs::File, io::Write};

use bevy::{
    app::{PluginGroupBuilder, ScheduleRunnerPlugin},
    diagnostic::DiagnosticsPlugin,
    log::LogPlugin,
    prelude::*,
    render::mesh::MeshPlugin,
    time::TimePlugin,
    utils::{HashMap, Instant}, gltf::GltfPlugin, scene::ScenePlugin,
};
use itertools::Itertools;
use once_cell::sync::Lazy;

use crate::{
    dcl::interface::{CrdtStore, CrdtType},
    dcl_component::{
        transform_and_parent::DclTransformAndParent, DclReader, DclWriter, SceneComponentId,
        SceneCrdtTimestamp, SceneEntityId,
    },
    ipfs::{IpfsIoPlugin, SceneIpfsLocation},
    scene_runner::{
        process_lifecycle, receive_scene_updates, send_scene_updates, update_scene_priority,
        update_world::{
            transform_and_parent::process_transform_and_parent_updates, CrdtLWWStateComponent,
        },
        LoadSceneEvent, RendererSceneContext, SceneEntity, SceneLoopSchedule, SceneRunnerPlugin,
        SceneUpdates,
    },
};

use super::PrimaryCamera;

pub struct TestPlugins;

pub static LOG_ADDED: Lazy<Mutex<bool>> = Lazy::new(|| Default::default());

impl PluginGroup for TestPlugins {
    fn build(self) -> PluginGroupBuilder {
        let builder = PluginGroupBuilder::start::<Self>();

        let mut log_added = LOG_ADDED.lock().unwrap();
        let builder = if !*log_added {
            *log_added = true;
            builder.add(LogPlugin::default())
        } else {
            builder
        };

        builder
            .add(TaskPoolPlugin::default())
            .add(TypeRegistrationPlugin::default())
            .add(FrameCountPlugin::default())
            .add(TimePlugin::default())
            .add(ScheduleRunnerPlugin::default())
            .add(TransformPlugin::default())
            .add(HierarchyPlugin::default())
            .add(DiagnosticsPlugin::default())
            .add(IpfsIoPlugin {
                server_prefix: Default::default(),
            })
            .add(AssetPlugin::default())
            .add(MeshPlugin)
            .add(GltfPlugin)
            .add(ScenePlugin)
    }
}

fn init_test_app(script: &str) -> App {
    let mut app = App::new();

    // Add our systems
    app.add_plugins(TestPlugins);
    app.add_asset::<Shader>();
    app.add_plugin(MaterialPlugin::<StandardMaterial>::default());
    app.add_plugin(SceneRunnerPlugin);

    // copy path so we can pass it into the closure
    let path = script.to_owned();

    // startup system to create camera and fire load event
    app.add_startup_system(
        move |mut commands: Commands, mut ev: EventWriter<LoadSceneEvent>| {
            commands.spawn((Camera3dBundle::default(), PrimaryCamera));
            ev.send(LoadSceneEvent {
                location: SceneIpfsLocation::Js(path.clone()),
            })
        },
    );

    // replace the scene loop schedule with a dummy so we can better control it
    app.world.remove_resource::<SceneLoopSchedule>().unwrap();
    app.world.insert_resource(SceneLoopSchedule {
        schedule: Schedule::new(),
        end_time: Instant::now(),
    });

    // run app once to get the scene initialized
    let mut q = app.world.query::<&RendererSceneContext>();
    while q.get_single(&mut app.world).is_err() {
        app.update();
    }

    app
}

// check output vs file
#[cfg(not(feature = "gen-tests"))]
macro_rules! assert_output_eq {
    ($result:ident, $path:expr) => {
        assert_eq!(
            $result.replace("\r", ""),
            include_str!($path).replace("\r", "")
        )
    };
}

// write output to file
#[allow(dead_code)]
fn write_expected(expected: String, filename: &str) {
    let mut path = PathBuf::from(file!());
    path.pop();
    path.push(filename);
    let mut f = File::create(path.clone()).unwrap();
    f.write_all(expected.as_bytes()).unwrap();
}

macro_rules! check_or_write {
    ($testdata:ident, $filename:expr) => {
        #[cfg(feature = "gen-tests")]
        write_expected($testdata, $filename);
        #[cfg(not(feature = "gen-tests"))]
        assert_output_eq!($testdata, $filename);
    };
}

fn make_graph(app: &mut App) -> String {
    let mut scene_query = app
        .world
        .query_filtered::<Entity, With<RendererSceneContext>>();
    assert_eq!(scene_query.iter(&app.world).len(), 1);
    let root = scene_query.iter(&app.world).next().unwrap();

    let mut scene_entity_query = app.world.query::<(&SceneEntity, Option<&Children>)>();
    let mut graph_nodes = HashMap::default();
    let mut graph = petgraph::Graph::<_, ()>::new();
    let mut to_check = vec![root];

    while let Some(ent) = to_check.pop() {
        debug!("current: {ent:?}, to_check: {to_check:?}");
        let Ok((scene_entity, maybe_children)) = scene_entity_query.get(&app.world, ent) else {
            panic!()
        };
        assert_eq!(scene_entity.root, root);

        let graph_node = *graph_nodes
            .entry(ent)
            .or_insert_with(|| graph.add_node(scene_entity.id.to_string()));

        if let Some(children) = maybe_children {
            let sorted_children_with_scene_id: BTreeMap<_, _> = children
                .iter()
                .map(|c| (scene_entity_query.get(&app.world, *c).unwrap().0.id, c))
                .collect();

            to_check.extend(sorted_children_with_scene_id.values().copied());
            for (child_id, child_ent) in sorted_children_with_scene_id.into_iter() {
                debug!(
                    "child of {:?}/{} -> {:?}/{}",
                    ent, scene_entity.id, child_ent, child_id
                );
                let child_graph_node = *graph_nodes
                    .entry(*child_ent)
                    .or_insert_with(|| graph.add_node(child_id.to_string()));
                graph.add_edge(graph_node, child_graph_node, ());
            }
        }
    }

    let dot = petgraph::dot::Dot::with_config(&graph, &[petgraph::dot::Config::EdgeNoLabel]);
    format!("{:?}", dot)
}

fn make_reparent_buffer(parent: u16) -> Vec<u8> {
    let parent = SceneEntityId {
        id: parent,
        generation: 0,
    };
    let mut buf = Vec::new();
    DclWriter::new(&mut buf).write(&DclTransformAndParent {
        parent,
        ..Default::default()
    });
    buf
}

fn run_single_update(app: &mut App) {
    // run once
    while app.world.resource_mut::<SceneUpdates>().jobs_in_flight == 0 {
        // set last update time to zero so the scheduler doesn't freak out
        app.world
            .query::<&mut RendererSceneContext>()
            .single_mut(&mut app.world)
            .last_sent = 0.0;
        Schedule::new()
            .add_systems((update_scene_priority, send_scene_updates).chain())
            .run(&mut app.world);
    }
    assert_eq!(app.world.resource_mut::<SceneUpdates>().jobs_in_flight, 1);

    while app.world.resource_mut::<SceneUpdates>().jobs_in_flight == 1 {
        // run the receiver and lifecycle part of the schedule
        Schedule::new()
            .add_systems(
                (
                    receive_scene_updates,
                    process_lifecycle,
                    apply_system_buffers,
                    process_transform_and_parent_updates,
                )
                    .chain(),
            )
            .run(&mut app.world);
    }

    // make sure we got the one response
    assert_eq!(app.world.resource_mut::<SceneUpdates>().jobs_in_flight, 0);
}

// basic hierarchy test
#[test]
fn flat_hierarchy() {
    // Setup app
    let mut app = init_test_app("tests/flat_hierarchy");

    let graph = make_graph(&mut app);
    check_or_write!(graph, "expected/flat_hierarchy_onStart.dot");

    info!("running update");

    // onUpdate
    run_single_update(&mut app);

    let graph = make_graph(&mut app);
    check_or_write!(graph, "expected/flat_hierarchy_onUpdate.dot");
}

// test moving entities out of a hierarchy
#[test]
fn reparenting() {
    // Setup app
    let mut app = init_test_app("tests/reparenting");

    // onUpdate
    run_single_update(&mut app);

    let graph = make_graph(&mut app);
    check_or_write!(graph, "expected/reparenting_1.dot");

    // onUpdate
    run_single_update(&mut app);
    let graph = make_graph(&mut app);
    check_or_write!(graph, "expected/reparenting_2.dot");
}

// test creating parents late
#[test]
fn late_entities() {
    // Setup app
    let mut app = init_test_app("tests/late_entities");

    // onUpdate
    run_single_update(&mut app);

    let graph = make_graph(&mut app);
    check_or_write!(graph, "expected/late_entities_1.dot");

    // onUpdate
    run_single_update(&mut app);
    let graph = make_graph(&mut app);
    check_or_write!(graph, "expected/late_entities_2.dot");

    // onUpdate
    run_single_update(&mut app);
    let graph = make_graph(&mut app);
    check_or_write!(graph, "expected/late_entities_3.dot");
}

#[test]
fn cyclic_recovery() {
    let states = [(3, 1), (1, 2), (2, 3), (3, 0)]
        .into_iter()
        .enumerate()
        .map(|(timestamp, (ent, par))| {
            (
                SceneEntityId {
                    id: ent,
                    generation: 0,
                },
                SceneCrdtTimestamp(timestamp as u32),
                make_reparent_buffer(par),
            )
        });

    for messages in states.permutations(4) {
        // create new app instance
        let mut app = init_test_app("tests/empty_scene");
        // add lww state
        let scene_entity = app
            .world
            .query_filtered::<Entity, With<RendererSceneContext>>()
            .single(&mut app.world);
        app.world
            .entity_mut(scene_entity)
            .insert(CrdtLWWStateComponent::<DclTransformAndParent>::default());

        let mut crdt_store = CrdtStore::default();

        for ix in 0..4 {
            let (dcl_entity, timestamp, data) = &messages[ix];
            let (mut scene_context, mut crdt_state) = app
                .world
                .query::<(
                    &mut RendererSceneContext,
                    &mut CrdtLWWStateComponent<DclTransformAndParent>,
                )>()
                .single_mut(&mut app.world);

            // initialize the scene entity
            if scene_context.bevy_entity(*dcl_entity).is_none() {
                scene_context.nascent.insert(*dcl_entity);
            }

            // add next message
            let reader = &mut DclReader::new(&data);
            crdt_store.try_update(
                SceneComponentId::TRANSFORM,
                CrdtType::LWW_ENT,
                *dcl_entity,
                *timestamp,
                Some(reader),
            );
            // pull updates
            *crdt_state = CrdtLWWStateComponent::new(
                crdt_store
                    .take_updates()
                    .lww
                    .get(&SceneComponentId::TRANSFORM)
                    .cloned()
                    .unwrap_or_default(),
            );

            // run systems
            Schedule::new()
                .add_systems(
                    (
                        process_lifecycle,
                        apply_system_buffers,
                        process_transform_and_parent_updates,
                    )
                        .chain(),
                )
                .run(&mut app.world);
        }
        let graph = make_graph(&mut app);
        check_or_write!(graph, "expected/cyclic_recovery.dot");
    }
}
