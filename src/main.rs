#![cfg_attr(not(feature = "console"), windows_subsystem = "windows")]
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

use std::{fs::File, io::Write, sync::OnceLock};

use analytics::{metrics::MetricsPlugin, segment_system::SegmentConfig};
use build_time::build_time_utc;
use imposters::DclImposterPlugin;
use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

use avatar::AvatarDynamicState;
use bevy::{
    asset::LoadState,
    core::TaskPoolThreadAssignmentPolicy,
    core_pipeline::{
        bloom::BloomSettings,
        prepass::{DepthPrepass, NormalPrepass},
        tonemapping::{DebandDither, Tonemapping},
        Skybox,
    },
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    pbr::{CascadeShadowConfigBuilder, ShadowFilteringMethod},
    prelude::*,
    render::{
        render_resource::{TextureViewDescriptor, TextureViewDimension},
        view::{ColorGrading, ColorGradingGlobal, ColorGradingSection, RenderLayers},
    },
    tasks::{IoTaskPool, Task},
    window::WindowResolution,
};
use bevy_console::ConsoleCommand;

use collectibles::CollectiblesPlugin;
use common::{
    sets::SetupSets,
    structs::{
        AppConfig, AttachPoints, GraphicsSettings, IVec2Arg, PrimaryCamera, PrimaryCameraRes,
        PrimaryPlayerRes, PrimaryUser, SceneImposterBake, SceneLoadDistance, Version,
        GROUND_RENDERLAYER, PRIMARY_AVATAR_LIGHT_LAYER,
    },
    util::{config_file, project_directories, TaskExt, UtilsPlugin},
};
use restricted_actions::{lookup_portable, RestrictedActionsPlugin};
use scene_material::SceneBoundPlugin;
use scene_runner::{
    automatic_testing::AutomaticTestingPlugin,
    initialize_scene::{PortableScenes, PortableSource, TestingData, PARCEL_SIZE},
    update_world::{mesh_collider::GroundCollider, NoGltf},
    OutOfWorld, SceneRunnerPlugin,
};

use av::AudioPlugin;
use avatar::AvatarPlugin;
use comms::{preview::PreviewMode, CommsPlugin};
use console::{ConsolePlugin, DoAddConsoleCommand};
use input_manager::InputManagerPlugin;
use ipfs::{IpfsAssetServer, IpfsIoPlugin};
use nft::{asset_source::NftReaderPlugin, NftShapePlugin};
use social::SocialPlugin;
use system_bridge::{NativeUi, SystemBridgePlugin};
use system_ui::{crash_report::CrashReportPlugin, SystemUiPlugin};
use tween::TweenPlugin;
use ui_core::UiCorePlugin;
use user_input::UserInputPlugin;
use uuid::Uuid;
use visuals::VisualsPlugin;
use wallet::WalletPlugin;
use world_ui::WorldUiPlugin;

static SESSION_LOG: OnceLock<String> = OnceLock::new();

pub fn version() -> String {
    format!(
        "{}{}",
        env!("BEVY_EXPLORER_VERSION"),
        (env!("BEVY_EXPLORER_LOCAL_MODIFICATION") == "true")
            .then_some(format!("-{}", build_time_utc!("%Y-%m-%d %H:%M")))
            .unwrap_or_default()
    )
}

fn main() {
    let session_time: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
    let dirs = project_directories();
    let log_dir = dirs.data_local_dir();
    let session_log = log_dir.join(format!("{}.log", session_time.format("%Y%m%d-%H%M%S")));
    SESSION_LOG
        .set(session_log.to_string_lossy().into_owned())
        .unwrap();
    std::fs::create_dir_all(log_dir).unwrap();

    let crash_file = std::fs::read_dir(log_dir)
        .unwrap()
        .filter_map(|f| f.ok())
        .find(|f| f.path().extension().map(|oss| oss.to_string_lossy()) == Some("touch".into()))
        .map(|f| {
            f.path()
                .parent()
                .unwrap()
                .join(f.path().file_stem().unwrap())
        });

    let mut args = pico_args::Arguments::from_env();

    File::create(SESSION_LOG.get().unwrap())
        .expect("failed to create log file")
        .write_all(format!("{}\n\n", SESSION_LOG.get().unwrap()).as_bytes())
        .expect("failed to create log file");

    File::create(format!("{}.touch", SESSION_LOG.get().unwrap())).unwrap();
    println!("log file: {}", SESSION_LOG.get().unwrap());

    // warnings before log init must be stored and replayed later
    let mut infos = Vec::default();
    let mut warnings = Vec::default();
    let mut app = App::new();

    let config_file = config_file();
    let base_config: AppConfig = std::fs::read(&config_file)
        .ok()
        .and_then(|f| {
            infos.push(format!("config file loaded from {:?}", config_file));
            serde_json::from_slice(&f)
                .map_err(|e| warnings.push(format!("failed to parse config.json: {e}")))
                .ok()
        })
        .unwrap_or_else(|| {
            warnings.push(format!(
                "config file not found at {:?}, generating default",
                config_file
            ));
            Default::default()
        });

    let final_config = AppConfig {
        server: args
            .value_from_str("--server")
            .ok()
            .unwrap_or(base_config.server),
        location: args
            .value_from_str::<_, IVec2Arg>("--location")
            .ok()
            .map(|va| va.0)
            .unwrap_or(base_config.location),
        previous_login: base_config.previous_login,
        graphics: GraphicsSettings {
            vsync: args
                .value_from_str("--vsync")
                .ok()
                .unwrap_or(base_config.graphics.vsync),
            log_fps: args
                .value_from_str("--log_fps")
                .ok()
                .unwrap_or(base_config.graphics.log_fps),
            fps_target: args
                .value_from_str::<_, usize>("--fps")
                .ok()
                .unwrap_or(base_config.graphics.fps_target),
            ..base_config.graphics
        },
        scene_threads: args
            .value_from_str("--threads")
            .ok()
            .unwrap_or(base_config.scene_threads),
        scene_load_distance: args
            .value_from_str("--distance")
            .ok()
            .unwrap_or(base_config.scene_load_distance),
        scene_unload_extra_distance: args
            .value_from_str("--unload")
            .ok()
            .unwrap_or(base_config.scene_unload_extra_distance),
        scene_imposter_bake: args
            .value_from_str("--bake")
            .ok()
            .map(|bake: String| match bake.to_lowercase().chars().next() {
                None | Some('f') => SceneImposterBake::FullSpeed,
                Some('h') => SceneImposterBake::HalfSpeed,
                Some('q') => SceneImposterBake::QuarterSpeed,
                Some('o') => SceneImposterBake::Off,
                _ => panic!(),
            })
            .unwrap_or(SceneImposterBake::Off),
        scene_imposter_distances: args
            .value_from_str("--impost")
            .ok()
            .map(|distances: String| {
                distances
                    .split(",")
                    .map(str::parse::<f32>)
                    .collect::<Result<Vec<f32>, _>>()
                    .unwrap()
            })
            .unwrap_or(base_config.scene_imposter_distances)
            .into_iter()
            .enumerate()
            .map(|(ix, d)| {
                let edge_distance = (1 << ix) as f32 * PARCEL_SIZE;
                let diagonal_distance = (edge_distance * edge_distance * 2.0).sqrt();
                println!("[{ix}] -> {}", d.max(diagonal_distance));
                d.max(diagonal_distance)
            })
            .collect(),
        scene_imposter_multisample: args
            .value_from_str("--impost_multi")
            .ok()
            .unwrap_or(base_config.scene_imposter_multisample),
        sysinfo_visible: args.contains("--sysinfo"),
        scene_log_to_console: args.contains("--scene_log_to_console"),
        ..base_config
    };

    let test_scenes = args.value_from_str("--test_scenes").ok();
    let test_mode = args.contains("--testing") || test_scenes.is_some();

    app.insert_resource(TestingData {
        inspect_hash: args.value_from_str("--inspect").ok(),
        test_mode,
        test_scenes: test_scenes.clone(),
    });

    let no_avatar = args.contains("--no_avatar");
    let no_gltf = args.contains("--no_gltf");
    let no_fog = args.contains("--no_fog");

    let is_preview = args.contains("--preview");

    let ui_scene: Option<String> = args.value_from_str("--ui").ok();
    if let Some(source) = ui_scene {
        app.add_systems(Update, spawn_system_ui_scene);
        app.insert_resource(NativeUi { login: false });
        app.insert_resource(SystemScene {
            source: Some(source),
        });
    } else {
        app.insert_resource(NativeUi { login: true });
    }

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

    let present_mode = match final_config.graphics.vsync {
        true => bevy::window::PresentMode::AutoVsync,
        false => bevy::window::PresentMode::AutoNoVsync,
    };

    let version_hash = version();
    let version = format!("{VERSION} ({version_hash})");

    app.insert_resource(Version(version.clone()))
        .insert_resource(final_config.audio.clone())
        .add_plugins(
            DefaultPlugins
                .set(TaskPoolPlugin {
                    task_pool_options: TaskPoolOptions {
                        async_compute: TaskPoolThreadAssignmentPolicy {
                            min_threads: 2,
                            max_threads: 8,
                            percent: 0.25,
                        },
                        io: TaskPoolThreadAssignmentPolicy {
                            min_threads: 8,
                            max_threads: 8,
                            percent: 0.25,
                        },
                        compute: TaskPoolThreadAssignmentPolicy {
                            min_threads: 2,
                            max_threads: 8,
                            percent: 0.25,
                        },
                        ..Default::default()
                    },
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Decentraland Bevy Explorer".to_owned(),
                        present_mode,
                        resolution: WindowResolution::new(1280.0, 720.0)
                            .with_scale_factor_override(1.0),
                        ..Default::default()
                    }),
                    ..Default::default()
                })
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
                .build()
                .add_before::<bevy::asset::AssetPlugin, _>(IpfsIoPlugin {
                    preview: is_preview,
                    starting_realm: Some(final_config.server.clone()),
                    assets_root: Default::default(),
                    num_slots: final_config.max_concurrent_remotes,
                })
                .add_before::<IpfsIoPlugin, _>(NftReaderPlugin),
        );

    if final_config.graphics.log_fps || is_preview {
        app.add_plugins(FrameTimeDiagnosticsPlugin);
    }
    if final_config.graphics.log_fps {
        app.add_plugins(LogDiagnosticsPlugin::default());
    }

    // Analytics
    app.add_plugins(MetricsPlugin);
    app.insert_resource(SegmentConfig::new(
        final_config.user_id.clone(),
        Uuid::new_v4().to_string(),
        version_hash,
    ));

    app.insert_resource(PreviewMode {
        server: is_preview.then_some(final_config.server.clone()),
        is_preview,
    });

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
    if no_gltf {
        app.insert_resource(NoGltf(true));
    }

    app.configure_sets(Startup, SetupSets::Init.before(SetupSets::Main));

    app.add_plugins(UtilsPlugin)
        .add_plugins(InputManagerPlugin)
        .add_plugins(SceneBoundPlugin)
        .add_plugins(SceneRunnerPlugin)
        .add_plugins(UserInputPlugin)
        .add_plugins(UiCorePlugin)
        .add_plugins(SystemUiPlugin)
        .add_plugins(ConsolePlugin { add_egui: true })
        .add_plugins(VisualsPlugin { no_fog })
        .add_plugins(WalletPlugin)
        .add_plugins(CommsPlugin)
        .add_plugins(SocialPlugin)
        .add_plugins(NftShapePlugin)
        .add_plugins(TweenPlugin)
        .add_plugins(CollectiblesPlugin)
        .add_plugins(WorldUiPlugin)
        .add_plugins(DclImposterPlugin)
        .add_plugins(SystemBridgePlugin { bare: false });

    if let Some(crashed) = crash_file {
        app.add_plugins(CrashReportPlugin {
            file: crashed.canonicalize().unwrap(),
        });
    }

    if !no_avatar {
        app.add_plugins(AvatarPlugin);
    }

    if test_scenes.is_some() {
        app.add_plugins(AutomaticTestingPlugin);
    }

    app.add_plugins(AudioPlugin)
        .add_plugins(RestrictedActionsPlugin)
        .insert_resource(PrimaryPlayerRes(Entity::PLACEHOLDER))
        .insert_resource(PrimaryCameraRes(Entity::PLACEHOLDER))
        .add_systems(Startup, setup.in_set(SetupSets::Init))
        .add_systems(Update, asset_loaded)
        .insert_resource(AmbientLight {
            color: Color::srgb(0.85, 0.85, 1.0),
            brightness: 575.0,
        });

    app.add_console_command::<ChangeLocationCommand, _>(change_location);
    app.add_console_command::<SceneDistanceCommand, _>(scene_distance);
    app.add_console_command::<SceneThreadsCommand, _>(scene_threads);
    app.add_console_command::<FpsCommand, _>(set_fps);

    info!("Bevy-Explorer version {}", version);

    // replay any logs
    for info in infos {
        info!("{}", info);
    }
    for warning in warnings {
        warn!(warning);
    }

    // requires local version of `bevy_mod_debugdump` due to once_cell version conflict.
    // probably resolved by updating deno. TODO: add feature flag for this after bumping deno
    // bevy_mod_debugdump::print_main_schedule(&mut app);
    #[cfg(not(feature = "console"))]
    log_panics::init();

    app.run();

    let _ = std::fs::remove_file(format!("{}.touch", SESSION_LOG.get().unwrap()));
}

fn setup(
    mut commands: Commands,
    mut player_resource: ResMut<PrimaryPlayerRes>,
    mut cam_resource: ResMut<PrimaryCameraRes>,
    config: Res<AppConfig>,
    asset_server: Res<AssetServer>,
) {
    info!("main::setup");
    // create the main player
    let attach_points = AttachPoints::new(&mut commands);
    let player_id = commands
        .spawn((
            SpatialBundle {
                transform: Transform::from_translation(Vec3::new(
                    8.0 + 16.0 * config.location.x as f32,
                    8.0,
                    -8.0 + -16.0 * config.location.y as f32,
                )),
                ..Default::default()
            },
            config.player_settings.clone(),
            OutOfWorld,
            AvatarDynamicState::default(),
            GroundCollider::default(),
        ))
        .push_children(&attach_points.entities())
        .insert(attach_points)
        .id();

    let skybox = asset_server.load("images/skybox/skybox_cubemap.png");

    // add a camera
    let camera_id = commands
        .spawn((
            Camera3dBundle {
                camera: Camera {
                    hdr: true,
                    ..Default::default()
                },
                tonemapping: Tonemapping::TonyMcMapface,
                deband_dither: DebandDither::Enabled,
                color_grading: ColorGrading {
                    // exposure: -0.5,
                    // gamma: 1.5,
                    // pre_saturation: 1.0,
                    // post_saturation: 1.0,
                    global: ColorGradingGlobal {
                        exposure: -0.5,
                        ..default()
                    },
                    shadows: ColorGradingSection {
                        gamma: 0.75,
                        ..Default::default()
                    },
                    midtones: ColorGradingSection {
                        gamma: 0.75,
                        ..Default::default()
                    },
                    highlights: ColorGradingSection {
                        gamma: 0.75,
                        ..Default::default()
                    },
                },
                projection: PerspectiveProjection {
                    // projection: OrthographicProjection {
                    far: 100000.0,
                    ..Default::default()
                }
                .into(),
                ..Default::default()
            },
            BloomSettings {
                intensity: 0.15,
                ..BloomSettings::OLD_SCHOOL
            },
            ShadowFilteringMethod::Gaussian,
            PrimaryCamera::default(),
            DepthPrepass,
            NormalPrepass,
            Skybox {
                image: skybox.clone(),
                brightness: 1000.0,
            },
            GROUND_RENDERLAYER.with(0),
        ))
        .id();

    commands.insert_resource(Cubemap {
        is_loaded: false,
        image_handle: skybox,
    });

    player_resource.0 = player_id;
    cam_resource.0 = camera_id;

    // add a directional light so it looks nicer
    commands.spawn((
        DirectionalLightBundle {
            directional_light: DirectionalLight {
                color: Color::srgb(1.0, 1.0, 0.7),
                shadows_enabled: true,
                ..Default::default()
            },
            transform: Transform::default().looking_at(Vec3::new(0.2, -0.5, -1.0), Vec3::Y),
            cascade_shadow_config: CascadeShadowConfigBuilder {
                maximum_distance: 100.0,
                ..Default::default()
            }
            .into(),
            ..Default::default()
        },
        RenderLayers::default().union(&PRIMARY_AVATAR_LIGHT_LAYER),
    ));
}

#[derive(Resource)]
struct Cubemap {
    is_loaded: bool,
    image_handle: Handle<Image>,
}

fn asset_loaded(
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut cubemap: ResMut<Cubemap>,
) {
    if !cubemap.is_loaded && asset_server.load_state(&cubemap.image_handle) == LoadState::Loaded {
        let image = images.get_mut(&cubemap.image_handle).unwrap();
        // NOTE: PNGs do not have any metadata that could indicate they contain a cubemap texture,
        // so they appear as one texture. The following code reconfigures the texture as necessary.
        image.reinterpret_stacked_2d_as_array(image.height() / image.width());
        image.texture_view_descriptor = Some(TextureViewDescriptor {
            dimension: Some(TextureViewDimension::Cube),
            ..default()
        });

        cubemap.is_loaded = true;
    }
}

// TODO move these somewhere better
/// set location
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/teleport")]
struct ChangeLocationCommand {
    #[arg(allow_hyphen_values(true))]
    x: i32,
    #[arg(allow_hyphen_values(true))]
    y: i32,
}

fn change_location(
    mut commands: Commands,
    mut input: ConsoleCommand<ChangeLocationCommand>,
    mut player: Query<(Entity, &mut Transform), With<PrimaryUser>>,
) {
    if let Some(Ok(command)) = input.take() {
        if let Ok((ent, mut transform)) = player.get_single_mut() {
            transform.translation.x = command.x as f32 * 16.0 + 8.0;
            transform.translation.z = -command.y as f32 * 16.0 - 8.0;
            if let Some(mut commands) = commands.get_entity(ent) {
                commands.try_insert(OutOfWorld);
            }
            input.reply_ok(format!("new location: {:?}", (command.x, command.y)));
            return;
        }

        input.reply_failed("failed to set location");
    }
}

/// set scene load distance (defaults to 75.0m) and additional unload distance (defaults to 25.0m)
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/scene_distance")]
struct SceneDistanceCommand {
    distance: Option<f32>,
    unload: Option<f32>,
}

fn scene_distance(
    mut input: ConsoleCommand<SceneDistanceCommand>,
    mut scene_load_distance: ResMut<SceneLoadDistance>,
) {
    if let Some(Ok(command)) = input.take() {
        let distance = command.distance.unwrap_or(75.0);
        scene_load_distance.load = distance;
        if let Some(unload) = command.unload {
            scene_load_distance.unload = unload;
        }
        input.reply_ok(format!(
            "set scene load distance to +{distance} -{}",
            scene_load_distance.load + scene_load_distance.unload
        ));
    }
}

// set thread count
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/scene_threads")]
struct SceneThreadsCommand {
    threads: Option<usize>,
}

fn scene_threads(mut input: ConsoleCommand<SceneThreadsCommand>, mut config: ResMut<AppConfig>) {
    if let Some(Ok(command)) = input.take() {
        let threads = command.threads.unwrap_or(4);
        config.scene_threads = threads;
        input.reply_ok("scene simultaneous thread count set to {threads}");
    }
}

// set fps
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/fps")]
struct FpsCommand {
    fps: usize,
}

fn set_fps(mut input: ConsoleCommand<FpsCommand>, mut config: ResMut<AppConfig>) {
    if let Some(Ok(command)) = input.take() {
        let fps = command.fps;
        config.graphics.fps_target = fps;
        input.reply_ok("target frame rate set to {fps}");
    }
}

#[derive(Resource)]
pub struct SystemScene {
    pub source: Option<String>,
}

#[allow(clippy::type_complexity)]
pub fn spawn_system_ui_scene(
    system_scene: Res<SystemScene>,
    mut task: Local<Option<Task<Result<(String, PortableSource), String>>>>,
    mut done: Local<bool>,
    mut portables: ResMut<PortableScenes>,
    ipfas: IpfsAssetServer,
) {
    if *done || system_scene.source.is_none() {
        return;
    }

    if task.is_none() {
        *task = Some(IoTaskPool::get().spawn(lookup_portable(
            None,
            system_scene.source.clone().unwrap(),
            true,
            Some(ipfas.ipfs().clone()),
        )));
    }

    let mut t = task.take().unwrap();
    match t.complete() {
        Some(Ok((hash, source))) => {
            info!("added ui scene from {}", source.pid);
            portables.0.extend([(hash, source)]);
            *done = true;
        }
        Some(Err(e)) => {
            error!("failed to load ui scene: {e}");
            *done = true;
        }
        None => *task = Some(t),
    }
}
