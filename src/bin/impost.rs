use std::{fs::File, io::Write, sync::OnceLock};

use comms::{
    profile::{CurrentUserProfile, ProfileCache, UserProfile},
    CommsPlugin,
};
use console::ConsolePlugin;

use dcl_deno::init_runtime;

use imposters::{
    render::{RetryImposter, SceneImposter},
    DclImposterPlugin,
};

use bevy::{
    app::{ScheduleRunnerPlugin, TaskPoolThreadAssignmentPolicy},
    prelude::*,
    window::ExitCondition,
    winit::WinitPlugin,
};

use common::{
    inputs::InputMap,
    profile::SerializedProfile,
    rpc::RpcCall,
    sets::SetupSets,
    structs::{
        AppConfig, AppError, AvatarDynamicState, CursorLocks, GraphicsSettings, IVec2Arg,
        PermissionUsed, PreviewMode, PrimaryCamera, PrimaryCameraRes, PrimaryPlayerRes,
        SceneGlobalLight, SceneImposterBake, SceneLoadDistance, SystemAudio, TimeOfDay, ToolTips,
    },
    util::UtilsPlugin,
};
use input_manager::{CumulativeAxisData, InputPriorities};
use nft::asset_source::Nft;
use restricted_actions::RestrictedActionsPlugin;
use scene_material::SceneBoundPlugin;
use scene_runner::{
    initialize_scene::ScenePointers, permissions::PermissionManager,
    update_world::mesh_collider::GroundCollider, OutOfWorld, SceneRunnerPlugin,
};

use ipfs::{map_realm_name, CurrentRealm, IpfsIoPlugin};
use system_bridge::SystemBridgePlugin;
use ui_core::{scrollable::ScrollTargetEvent, UiCorePlugin};
use wallet::Wallet;

static SESSION_LOG: OnceLock<String> = OnceLock::new();

fn main() {
    let session_time: chrono::DateTime<chrono::Utc> = chrono::DateTime::from_timestamp_millis(
        web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64,
    )
    .unwrap();
    let dirs = platform::project_directories().unwrap();
    let log_dir = dirs.data_local_dir();
    let session_log = log_dir.join(format!(
        "impost-{}.log",
        session_time.format("%Y%m%d-%H%M%S")
    ));
    SESSION_LOG
        .set(session_log.to_string_lossy().into_owned())
        .unwrap();
    std::fs::create_dir_all(log_dir).unwrap();

    File::create(SESSION_LOG.get().unwrap())
        .expect("failed to create log file")
        .write_all(format!("{}\n\n", SESSION_LOG.get().unwrap()).as_bytes())
        .expect("failed to create log file");

    init_runtime();

    let mut args = pico_args::Arguments::from_env();
    let config_file = platform::project_directories()
        .unwrap()
        .config_dir()
        .join("config.json");

    let levels = args.value_from_str("--levels").unwrap_or(5);
    let range = args
        .value_from_str("--range")
        .map(|f: f32| f * 16.0)
        .unwrap_or(f32::MAX);

    let base_config: AppConfig = std::fs::read(&config_file)
        .ok()
        .and_then(|f| serde_json::from_slice(&f).ok())
        .unwrap_or_default();

    let final_config = AppConfig {
        server: args
            .value_from_str("--server")
            .ok()
            .unwrap_or(base_config.server),
        location: args
            .value_from_str::<_, IVec2Arg>("--location")
            .ok()
            .map(|va| va.0)
            .unwrap_or(IVec2::ZERO),
        graphics: GraphicsSettings {
            vsync: false,
            log_fps: false,
            fps_target: 999,
            ..base_config.graphics
        },
        scene_threads: args
            .value_from_str("--threads")
            .ok()
            .unwrap_or(base_config.scene_threads),
        scene_load_distance: -1.0,
        scene_unload_extra_distance: 0.0,
        scene_imposter_bake: SceneImposterBake::FullSpeed,
        scene_imposter_distances: std::iter::repeat_n(0.0, levels)
            .chain(std::iter::once(range))
            .collect(),
        scene_log_to_console: args.contains("--scene_log_to_console"),
        ..base_config
    };

    let content_server_override = args.value_from_str("--content-server").ok();
    let zip_output = args.value_from_str("--zip-output").ok();

    let no_download = args.contains("--no-download");

    let remaining = args.finish();
    if !remaining.is_empty() {
        println!(
            "failed to parse args: {}",
            remaining
                .iter()
                .map(|arg| arg.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ")
        );
        return;
    }

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(TaskPoolPlugin {
                task_pool_options: TaskPoolOptions {
                    async_compute: TaskPoolThreadAssignmentPolicy {
                        min_threads: 2,
                        max_threads: 8,
                        percent: 0.25,
                        on_thread_spawn: None,
                        on_thread_destroy: None,
                    },
                    io: TaskPoolThreadAssignmentPolicy {
                        min_threads: 8,
                        max_threads: 8,
                        percent: 0.25,
                        on_thread_spawn: None,
                        on_thread_destroy: None,
                    },
                    compute: TaskPoolThreadAssignmentPolicy {
                        min_threads: 2,
                        max_threads: 8,
                        percent: 0.25,
                        on_thread_spawn: None,
                        on_thread_destroy: None,
                    },
                    ..Default::default()
                },
            })
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
                ..Default::default()
            })
            .set(bevy::asset::AssetPlugin {
                // we manage asset server loads via ipfs module, so we don't need this protection
                unapproved_path_mode: bevy::asset::UnapprovedPathMode::Allow,
                ..Default::default()
            })
            .disable::<WinitPlugin>()
            .set(bevy::log::LogPlugin {
                filter: "wgpu=error,naga=error,bevy_animation=error,matrix=error".to_string(),
                custom_layer: |_| {
                    let (non_blocking, guard) = tracing_appender::non_blocking(
                        File::options()
                            .write(true)
                            .open(SESSION_LOG.get().unwrap())
                            .unwrap(),
                    );
                    Box::leak(guard.into());
                    Some(Box::new(
                        bevy::log::tracing_subscriber::fmt::layer()
                            .with_writer(non_blocking)
                            .with_ansi(false),
                    ))
                },
                ..default()
            })
            .add_before::<bevy::asset::AssetPlugin>(IpfsIoPlugin {
                preview: false,
                starting_realm: Some(map_realm_name(&final_config.server)),
                content_server_override,
                assets_root: Default::default(),
                num_slots: final_config.max_concurrent_remotes,
            }),
    );

    app.add_plugins(ScheduleRunnerPlugin::run_loop(
        // Run full speed
        std::time::Duration::ZERO,
    ));

    // Analytics
    app.insert_resource(SceneLoadDistance {
        load: final_config.scene_load_distance,
        unload: final_config.scene_unload_extra_distance,
        load_imposter: final_config
            .scene_imposter_distances
            .last()
            .map(|last| {
                // actual distance we need is last + diagonal of the largest mip size
                let mip_size =
                    (1 << (final_config.scene_imposter_distances.len() - 1)) as f32 * 16.0;
                let req = last + (2.0 * mip_size * mip_size).sqrt();
                println!(
                    "imposter mips: {:?} -> distance {}",
                    final_config.scene_imposter_distances, req
                );
                req
            })
            .unwrap_or(0.0),
    });

    app.insert_resource(final_config);

    app.configure_sets(Startup, SetupSets::Init.before(SetupSets::Main));

    app.add_plugins(UtilsPlugin)
        .add_plugins(UiCorePlugin)
        .add_plugins(ConsolePlugin { add_egui: true })
        .add_plugins(SceneBoundPlugin)
        .add_plugins(SceneRunnerPlugin)
        .add_plugins(CommsPlugin)
        .add_plugins(RestrictedActionsPlugin)
        .add_plugins(DclImposterPlugin {
            zip_output,
            download: !no_download,
        })
        .add_plugins(SystemBridgePlugin { bare: true });

    app.insert_resource(PrimaryPlayerRes(Entity::PLACEHOLDER))
        .insert_resource(PrimaryCameraRes(Entity::PLACEHOLDER))
        .add_systems(Startup, setup.in_set(SetupSets::Init));

    // add required things that don't get initialized by their plugins
    let mut wallet = Wallet::default();
    wallet.finalize_as_guest();
    app.init_resource::<ProfileCache>()
        .insert_resource(wallet)
        .add_event::<SystemAudio>()
        .init_resource::<PermissionManager>()
        .init_resource::<InputMap>()
        .init_resource::<InputPriorities>()
        .init_resource::<CumulativeAxisData>()
        .init_resource::<ToolTips>()
        .init_resource::<SceneGlobalLight>()
        .add_event::<RpcCall>()
        .add_event::<ScrollTargetEvent>()
        .add_event::<PermissionUsed>()
        .init_resource::<PreviewMode>()
        .init_asset::<Nft>()
        .init_resource::<CursorLocks>()
        .insert_resource(TimeOfDay {
            time: 10.0 * 3600.0,
            target_time: None,
            speed: 12.0,
        });

    // requires local version of `bevy_mod_debugdump` due to once_cell version conflict.
    // probably resolved by updating deno. TODO: add feature flag for this after bumping deno
    // bevy_mod_debugdump::print_main_schedule(&mut app);
    #[cfg(not(feature = "console"))]
    log_panics::init();

    app.add_systems(PreUpdate, check_done);

    app.run();
}

#[allow(clippy::type_complexity)]
fn check_done(
    q: Query<
        (),
        (
            With<SceneImposter>,
            Without<RetryImposter>,
            Without<Children>,
        ),
    >,
    realm: Res<CurrentRealm>,
    pointers: Res<ScenePointers>,
    mut counter: Local<usize>,
    config: Res<AppConfig>,
    mut exit: EventWriter<AppExit>,
    mut errors: EventReader<AppError>,
) {
    // wait for realm
    if realm.address.is_empty() {
        *counter = 0;
        return;
    }

    // wait for pointers
    if pointers.get(config.location).is_none() {
        *counter = 0;
        return;
    }
    if !pointers.is_full() {
        *counter = 0;
        return;
    }

    // wait till nothing missing
    if q.is_empty() {
        *counter += 1;
        if *counter == 10 {
            info!("all done!");
            exit.write_default();
        }
    } else {
        *counter = 0;
    }

    // or till a fatal error occurs
    let errors = errors.read().collect::<Vec<_>>();
    if !errors.is_empty() {
        for error in errors {
            error!("fatal error: {error:?}");
        }
        println!("failed!");
        exit.write(AppExit::from_code(1));
    }
}

fn setup(
    mut commands: Commands,
    mut player_resource: ResMut<PrimaryPlayerRes>,
    mut cam_resource: ResMut<PrimaryCameraRes>,
    config: Res<AppConfig>,
    mut wallet: ResMut<Wallet>,
    mut current_profile: ResMut<CurrentUserProfile>,
) {
    info!("main::setup");
    // create the main player
    let player_id = commands
        .spawn((
            Transform::from_translation(Vec3::new(
                8.0 + 16.0 * config.location.x as f32,
                8.0,
                -8.0 + -16.0 * config.location.y as f32,
            )),
            Visibility::default(),
            config.player_settings.clone(),
            OutOfWorld,
            AvatarDynamicState::default(),
            GroundCollider::default(),
        ))
        .id();

    let camera_id = commands
        .spawn((Camera3d::default(), PrimaryCamera::default()))
        .id();

    player_resource.0 = player_id;
    cam_resource.0 = camera_id;

    wallet.finalize_as_guest();
    current_profile.profile = Some(UserProfile {
        version: 0,
        content: SerializedProfile {
            eth_address: format!("{:#x}", wallet.address().unwrap()),
            user_id: Some(format!("{:#x}", wallet.address().unwrap())),
            ..Default::default()
        },
        base_url: Default::default(),
    });
    current_profile.is_deployed = true;
}
