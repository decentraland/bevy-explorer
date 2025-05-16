use std::{path::PathBuf, sync::Mutex};

use std::{collections::BTreeMap, fs::File, io::Write};

use bevy::{
    app::{PluginGroupBuilder, ScheduleRunnerPlugin},
    diagnostic::DiagnosticsPlugin,
    gizmos::GizmoPlugin,
    gltf::GltfPlugin,
    input::InputPlugin,
    log::LogPlugin,
    prelude::*,
    render::mesh::MeshPlugin,
    scene::ScenePlugin,
    time::TimePlugin,
    utils::{HashMap, Instant},
};
use bevy_dui::DuiPlugin;
use itertools::Itertools;
use once_cell::sync::Lazy;
use scene_material::SceneBoundPlugin;
use spin_sleep::SpinSleeper;
use system_bridge::SystemBridgePlugin;
use ui_core::{scrollable::ScrollTargetEvent, stretch_uvs_image::StretchUvMaterial};

use crate::{
    initialize_scene::{PointerResult, ScenePointers},
    permissions::PermissionManager,
    process_scene_entity_lifecycle,
    update_world::{
        transform_and_parent::process_transform_and_parent_updates, CrdtStateComponent,
    },
    RendererSceneContext, SceneEntity, SceneLoopLabel, SceneLoopSchedule, SceneRunnerPlugin,
    SceneUpdates,
};
use common::{
    inputs::InputMap,
    rpc::RpcCall,
    structs::{
        AppConfig, CursorLocks, GraphicsSettings, PrimaryCamera, PrimaryPlayerRes,
        SceneGlobalLight, SceneLoadDistance, ToolTips,
    },
};
use comms::{preview::PreviewMode, CommsPlugin};
use console::{self, ConsolePlugin};
use dcl::{
    crdt::lww::CrdtLWWState,
    interface::{CrdtStore, CrdtType},
};
use dcl_component::{
    transform_and_parent::DclTransformAndParent, DclReader, DclWriter, SceneComponentId,
    SceneCrdtTimestamp, SceneEntityId,
};
use input_manager::{CumulativeAxisData, InputPriorities};
use ipfs::{IpfsIoPlugin, IpfsResource, ServerAbout, ServerConfiguration};
use wallet::WalletPlugin;

use super::{initialize_scene::SceneLoading, PrimaryUser};

pub struct TestPlugins;

pub static LOG_ADDED: Lazy<Mutex<bool>> = Lazy::new(Default::default);

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

        let mut test_path = std::env::current_dir().unwrap();
        test_path.push("src");
        test_path.push("test");
        test_path.push("test_assets");

        builder
            .add(TaskPoolPlugin::default())
            .add(TypeRegistrationPlugin)
            .add(FrameCountPlugin)
            .add(TimePlugin)
            .add(ScheduleRunnerPlugin::default())
            .add(TransformPlugin)
            .add(HierarchyPlugin)
            .add(DiagnosticsPlugin)
            .add(IpfsIoPlugin {
                preview: false,
                assets_root: test_path.to_str().map(ToOwned::to_owned),
                starting_realm: Default::default(),
                num_slots: 8,
                content_server_override: None,
            })
            .add(AssetPlugin::default())
            .add(MeshPlugin)
            .add(GltfPlugin::default())
            .add(AnimationPlugin)
            .add(InputPlugin)
            .add(ScenePlugin)
            .add(ConsolePlugin { add_egui: false })
            .add(WalletPlugin)
            .add(CommsPlugin)
            .add(DuiPlugin)
            .add(SystemBridgePlugin { bare: true })
    }
}

fn init_test_app(entity_json: &str) -> App {
    let mut app = App::new();

    // Add our systems
    app.insert_resource(AppConfig {
        graphics: GraphicsSettings {
            fps_target: 0,
            ..Default::default()
        },
        ..Default::default()
    });
    app.add_plugins(TestPlugins);
    app.init_asset::<Shader>();
    app.init_asset::<AnimationClip>();
    app.init_asset::<Image>();
    app.init_asset::<StretchUvMaterial>();
    app.add_plugins(MaterialPlugin::<StandardMaterial>::default());
    app.add_plugins(GizmoPlugin);
    app.add_plugins(SceneRunnerPlugin);
    app.add_plugins(SceneBoundPlugin);
    app.insert_resource(PrimaryPlayerRes(Entity::PLACEHOLDER));
    app.init_resource::<PermissionManager>();
    app.init_resource::<InputMap>();
    app.init_resource::<InputPriorities>();
    app.init_resource::<CumulativeAxisData>();
    app.init_resource::<ToolTips>();
    app.init_resource::<SceneGlobalLight>();
    app.add_event::<RpcCall>();
    app.add_event::<ScrollTargetEvent>();
    app.insert_resource(SceneLoadDistance {
        load: 1.0,
        unload: 0.0,
        load_imposter: 0.0,
    });
    app.init_resource::<PreviewMode>();
    app.init_resource::<CursorLocks>();
    app.finish();

    let mut test_path = std::env::current_dir().unwrap();
    test_path.push("src");
    test_path.push("test");
    test_path.push("test_assets");

    let ipfs = app.world().resource::<IpfsResource>();
    let urn = format!("urn:decentraland:entity:{entity_json}");
    ipfs.set_realm_about(ServerAbout {
        content: None,
        configurations: Some(ServerConfiguration {
            scenes_urn: Some(vec![urn.clone()]),
            local_scene_parcels: Some(vec!["0,0".to_owned()]),
            ..Default::default()
        }),
        ..Default::default()
    });

    app.world_mut().resource_mut::<ScenePointers>().insert(
        IVec2::ZERO,
        PointerResult::Exists {
            realm: "manual value".to_owned(),
            hash: "whatever".to_owned(),
            urn: Some(urn),
        },
    );

    // startup system to create camera and fire load event
    app.add_systems(Startup, move |mut commands: Commands| {
        commands.spawn((
            SpatialBundle::default(),
            PrimaryUser::default(),
            PrimaryCamera::default(),
        ));
    });

    // replace the scene loop schedule with a dummy so we can better control it
    app.world_mut()
        .remove_resource::<SceneLoopSchedule>()
        .unwrap();
    let mut skip_loop_schedule = Schedule::new(SceneLoopLabel);
    skip_loop_schedule.add_systems(|mut updates: ResMut<SceneUpdates>| {
        updates.eligible_jobs = 0;
    });
    app.world_mut().insert_resource(SceneLoopSchedule {
        schedule: skip_loop_schedule,
        prev_time: Instant::now(),
        run_time: 100.0,
        sleeper: SpinSleeper::default(),
    });

    // run app once to get the scene initialized
    let mut q = app
        .world_mut()
        .query_filtered::<&RendererSceneContext, Without<SceneLoading>>();
    while q.get_single(app.world_mut()).is_err() {
        app.update();
        // if let Ok(loading) = app.world.query::<&SceneLoading>().get_single(&mut app.world) {
        //     warn!("load state: {loading:?}");
        // }
        // if let Ok(context) = app.world.query::<&RendererSceneContext>().get_single(&mut app.world) {
        //     warn!("context tick: {:?} (blocked: {:?})", context.tick_number, context.blocked);
        // }
    }

    app.world_mut().insert_resource(SceneLoopSchedule {
        schedule: Schedule::new(SceneLoopLabel),
        prev_time: Instant::now(),
        run_time: 100.0,
        sleeper: SpinSleeper::default(),
    });

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
        .world_mut()
        .query_filtered::<Entity, With<RendererSceneContext>>();
    assert_eq!(scene_query.iter(app.world()).len(), 1);
    let root = scene_query.iter(app.world()).next().unwrap();

    let mut scene_entity_query = app.world_mut().query::<(&SceneEntity, Option<&Children>)>();
    let mut graph_nodes = HashMap::default();
    let mut graph = petgraph::Graph::<_, ()>::new();
    let mut to_check = vec![root];

    while let Some(ent) = to_check.pop() {
        debug!("current: {ent:?}, to_check: {to_check:?}");
        let Ok((scene_entity, maybe_children)) = scene_entity_query.get(app.world(), ent) else {
            panic!()
        };
        assert_eq!(scene_entity.root, root);

        let graph_node = *graph_nodes
            .entry(ent)
            .or_insert_with(|| graph.add_node(scene_entity.id.to_string()));

        if let Some(children) = maybe_children {
            let sorted_children_with_scene_id: BTreeMap<_, _> = children
                .iter()
                .filter_map(|c| {
                    scene_entity_query
                        .get(app.world(), *c)
                        .ok()
                        .map(|q| (q.0.id, c))
                })
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

// fn run_single_update(app: &mut App) {
//     // run once
//     while app
//         .world
//         .resource_mut::<SceneUpdates>()
//         .jobs_in_flight
//         .is_empty()
//     {
//         // set last update time to zero so the scheduler doesn't freak out
//         app.world
//             .query::<&mut RendererSceneContext>()
//             .single_mut(&mut app.world)
//             .last_sent = 0.0;
//         Schedule::new(SceneLoopLabel)
//             .add_systems((update_scene_priority, send_scene_updates).chain())
//             .run(&mut app.world);
//     }
//     assert_eq!(
//         app.world
//             .resource_mut::<SceneUpdates>()
//             .jobs_in_flight
//             .len(),
//         1
//     );

//     while app
//         .world
//         .resource_mut::<SceneUpdates>()
//         .jobs_in_flight
//         .len()
//         == 1
//     {
//         // run the receiver and lifecycle part of the schedule
//         Schedule::new(SceneLoopLabel)
//             .add_systems(
//                 (
//                     receive_scene_updates,
//                     process_scene_entity_lifecycle,
//                     apply_deferred,
//                     process_transform_and_parent_updates,
//                 )
//                     .chain(),
//             )
//             .run(&mut app.world);
//     }

//     // make sure we got the one response
//     assert!(app
//         .world
//         .resource_mut::<SceneUpdates>()
//         .jobs_in_flight
//         .is_empty());
// }

// basic hierarchy test
// #[test]
// fn flat_hierarchy() {
//     // Setup app
//     let mut app = init_test_app("flat_hierarchy");

//     let graph = make_graph(&mut app);
//     check_or_write!(graph, "expected/flat_hierarchy_onStart.dot");

//     info!("running update");

//     // onUpdate
//     run_single_update(&mut app);

//     let graph = make_graph(&mut app);
//     check_or_write!(graph, "expected/flat_hierarchy_onUpdate.dot");
// }

// // test moving entities out of a hierarchy
// #[test]
// fn reparenting() {
//     // Setup app
//     let mut app = init_test_app("reparenting");

//     // onUpdate
//     run_single_update(&mut app);

//     let graph = make_graph(&mut app);
//     check_or_write!(graph, "expected/reparenting_1.dot");

//     // onUpdate
//     run_single_update(&mut app);
//     let graph = make_graph(&mut app);
//     check_or_write!(graph, "expected/reparenting_2.dot");
// }

// // test creating parents late
// #[test]
// fn late_entities() {
//     // Setup app
//     let mut app = init_test_app("late_entities");

//     // onUpdate
//     run_single_update(&mut app);

//     let graph = make_graph(&mut app);
//     check_or_write!(graph, "expected/late_entities_1.dot");

//     // onUpdate
//     run_single_update(&mut app);
//     let graph = make_graph(&mut app);
//     check_or_write!(graph, "expected/late_entities_2.dot");

//     // onUpdate
//     run_single_update(&mut app);
//     let graph = make_graph(&mut app);
//     check_or_write!(graph, "expected/late_entities_3.dot");
// }

#[test]
fn cyclic_recovery() {
    let states = [(603, 601), (601, 602), (602, 603), (603, 0)]
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
        let mut app = init_test_app("empty_scene.entity_definition");
        // add lww state
        let scene_entity = app
            .world_mut()
            .query_filtered::<Entity, With<RendererSceneContext>>()
            .single(app.world_mut());
        app.world_mut()
            .entity_mut(scene_entity)
            .insert(CrdtStateComponent::<CrdtLWWState, DclTransformAndParent>::default());

        let mut crdt_store = CrdtStore::default();

        for (dcl_entity, timestamp, data) in messages.iter().take(4) {
            let (mut scene_context, mut crdt_state) = app
                .world_mut()
                .query::<(
                    &mut RendererSceneContext,
                    &mut CrdtStateComponent<CrdtLWWState, DclTransformAndParent>,
                )>()
                .single_mut(app.world_mut());

            // initialize the scene entity
            if scene_context.bevy_entity(*dcl_entity).is_none() {
                scene_context.nascent.insert(*dcl_entity);
            }

            // add next message
            let reader = &mut DclReader::new(data);
            crdt_store.try_update(
                SceneComponentId::TRANSFORM,
                CrdtType::LWW_ENT,
                *dcl_entity,
                *timestamp,
                Some(reader),
            );
            // pull updates
            *crdt_state = CrdtStateComponent::new(
                crdt_store
                    .take_updates()
                    .lww
                    .get(&SceneComponentId::TRANSFORM)
                    .cloned()
                    .unwrap_or_default(),
            );

            // run systems
            Schedule::new(SceneLoopLabel)
                .add_systems(
                    (
                        process_scene_entity_lifecycle,
                        apply_deferred,
                        process_transform_and_parent_updates,
                    )
                        .chain(),
                )
                .run(app.world_mut());
        }
        let graph = make_graph(&mut app);
        check_or_write!(graph, "expected/cyclic_recovery.dot");
    }
}

#[test]
fn test_scene_ray() {
    fn ray_code(mut position: Vec3, mut ray: Vec3) -> Vec<(IVec2, f32)> {
        let mut results = Vec::default();

        let mut distance = 0.0;

        if ray.length() == 0.0 {
            return results;
        }

        if ray.length() > 1000.0 {
            ray = ray.normalize() * 1000.0;
        }

        const EPS: f32 = 0.01;
        let offset: Vec3 = Vec3::new(ray.x.signum() * EPS, ray.y.signum() * EPS, 0.0);

        loop {
            let adj_position = position + offset;

            results.push((
                (adj_position / 16.0).floor().truncate().as_ivec2(),
                distance,
            ));

            let x_dist = if ray.x < 0.0 {
                (((adj_position.x / 16.0).floor() * 16.0) - position.x) / ray.x
            } else if ray.x > 0.0 {
                (((adj_position.x / 16.0).ceil() * 16.0) - position.x) / ray.x
            } else {
                999.0
            };
            let y_dist = if ray.y < 0.0 {
                (((adj_position.y / 16.0).floor() * 16.0) - position.y) / ray.y
            } else if ray.y > 0.0 {
                (((adj_position.y / 16.0).ceil() * 16.0) - position.y) / ray.y
            } else {
                999.0
            };
            println!("pos: {position}, ray: {ray}, x:{x_dist} / y:{y_dist}");

            let step_fraction = x_dist.min(y_dist);
            if step_fraction > 1.0 {
                return results;
            }

            let step = ray * step_fraction;
            position += step;
            distance += step.length();
            ray -= step;
            println!("step: {step}, dist: {distance}");
        }
    }

    assert_eq!(ray_code(Vec3::ONE, Vec3::ONE), vec![(IVec2::ZERO, 0.0)]);
    assert_eq!(
        ray_code(Vec3::splat(17.0), Vec3::ONE),
        vec![(IVec2::ONE, 0.0)]
    );
    assert_eq!(
        ray_code(Vec3::splat(-17.0), -Vec3::ONE),
        vec![(IVec2::splat(-2), 0.0)]
    );

    let results = ray_code(Vec3::splat(15.0), Vec3::new(2.0, 2.0, 0.0));
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].0, IVec2::splat(0));
    assert_eq!(results[1].0, IVec2::splat(1));
    assert_eq!(results[0].1, 0.0);
    assert!((results[1].1 - 2f32.sqrt()).abs() < 0.01);

    let results = ray_code(Vec3::splat(15.0), Vec3::new(2.0, 4.0, 0.0));
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].0, IVec2::splat(0));
    assert_eq!(results[1].0, IVec2::new(0, 1));
    assert_eq!(results[2].0, IVec2::splat(1));
    assert_eq!(results[0].1, 0.0);
    assert!((results[1].1 - f32::sqrt(1.0 + 0.5 * 0.5)).abs() < 0.01);
    assert!((results[2].1 - 2.0 * f32::sqrt(1.0 + 0.5 * 0.5)).abs() < 0.01);

    let results = ray_code(Vec3::splat(-15.0), -Vec3::new(2.0, 2.0, 0.0));
    println!("results: {results:?}");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].0, IVec2::splat(-1));
    assert_eq!(results[1].0, IVec2::splat(-2));
    assert_eq!(results[0].1, 0.0);
    assert!((results[1].1 - 2f32.sqrt()).abs() < 0.01);

    let results = ray_code(Vec3::splat(-15.0), -Vec3::new(2.0, 4.0, 0.0));
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].0, IVec2::splat(-1));
    assert_eq!(results[1].0, IVec2::new(-1, -2));
    assert_eq!(results[2].0, IVec2::splat(-2));
    assert_eq!(results[0].1, 0.0);
    assert!((results[1].1 - f32::sqrt(1.0 + 0.5 * 0.5)).abs() < 0.01);
    assert!((results[2].1 - 2.0 * f32::sqrt(1.0 + 0.5 * 0.5)).abs() < 0.01);

    let results = ray_code(Vec3::splat(-8.0), Vec3::new(8.0, -8.0, 0.0));
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].0, IVec2::splat(-1));
    assert_eq!(results[1].0, IVec2::new(0, -2));
    assert_eq!(results[0].1, 0.0);
    assert!((results[1].1 - f32::sqrt(8.0 * 8.0 * 2.0)).abs() < 0.01);
}
