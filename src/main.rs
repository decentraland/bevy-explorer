pub const VERSION: &str = env!("CARGO_PKG_VERSION");

use build_time::build_time_utc;
use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

use avatar::AvatarDynamicState;
use bevy::{
    core::TaskPoolThreadAssignmentPolicy,
    core_pipeline::{
        bloom::BloomSettings,
        tonemapping::{DebandDither, Tonemapping},
    },
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    pbr::{CascadeShadowConfigBuilder, ShadowFilteringMethod},
    prelude::*,
    render::view::ColorGrading,
    text::TextSettings,
};
use bevy_console::ConsoleCommand;

use collectibles::CollectiblesPlugin;
use common::{
    sets::SetupSets,
    structs::{
        AppConfig, AttachPoints, GraphicsSettings, IVec2Arg, PrimaryCamera, PrimaryCameraRes,
        PrimaryPlayerRes, PrimaryUser, SceneLoadDistance, Version,
    },
    util::UtilsPlugin,
};
use restricted_actions::RestrictedActionsPlugin;
use scene_material::SceneBoundPlugin;
use scene_runner::{
    automatic_testing::AutomaticTestingPlugin,
    initialize_scene::TestingData,
    update_world::{mesh_collider::GroundCollider, NoGltf},
    OutOfWorld, SceneRunnerPlugin,
};

use av::AudioPlugin;
use avatar::AvatarPlugin;
use comms::CommsPlugin;
use console::{ConsolePlugin, DoAddConsoleCommand};
use input_manager::InputManagerPlugin;
use ipfs::IpfsIoPlugin;
use nft::{asset_source::NftReaderPlugin, NftShapePlugin};
use system_ui::SystemUiPlugin;
use tween::TweenPlugin;
use ui_core::UiCorePlugin;
use user_input::UserInputPlugin;
use visuals::VisualsPlugin;
use wallet::WalletPlugin;
use world_ui::WorldUiPlugin;

fn main() {
    // warnings before log init must be stored and replayed later
    let mut warnings = Vec::default();
    let mut app = App::new();

    let base_config: AppConfig = std::fs::read("config.json")
        .ok()
        .and_then(|f| {
            serde_json::from_slice(&f)
                .map_err(|e| warnings.push(format!("failed to parse config.json: {e}")))
                .ok()
        })
        .unwrap_or_default();
    let mut args = pico_args::Arguments::from_env();

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
        sysinfo_visible: false,
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

    // std::fs::write(
    //     "config.json.out",
    //     serde_json::to_string(&final_config).unwrap(),
    // )
    // .unwrap();

    let present_mode = match final_config.graphics.vsync {
        true => bevy::window::PresentMode::AutoVsync,
        false => bevy::window::PresentMode::AutoNoVsync,
    };

    // let msaa = match final_config.graphics.msaa {
    //     1 => Msaa::Off,
    //     2 => Msaa::Sample2,
    //     4 => Msaa::Sample4,
    //     8 => Msaa::Sample8,
    //     _ => {
    //         warnings.push(
    //             "Invalid msaa sample count, must be one of (1, 2, 4, 8). Defaulting to Off"
    //                 .to_owned(),
    //         );
    //         Msaa::Off
    //     }
    // };

    let bt = build_time_utc!("%Y-%m-%d %H:%M");
    let version = format!("{VERSION} ({bt})");

    app //.insert_resource(msaa)
        .insert_resource(Version(version))
        .insert_resource(TextSettings {
            soft_max_font_atlases: 4.try_into().unwrap(),
            allow_dynamic_font_size: true,
        })
        .insert_resource(final_config.audio.clone())
        .add_plugins(
            DefaultPlugins
                .set(TaskPoolPlugin {
                    task_pool_options: TaskPoolOptions {
                        async_compute: TaskPoolThreadAssignmentPolicy {
                            min_threads: 1,
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
                        ..Default::default()
                    }),
                    ..Default::default()
                })
                .set(bevy::log::LogPlugin {
                    filter: "wgpu=error,naga=error".to_string(),
                    ..default()
                })
                .build()
                .add_before::<bevy::asset::AssetPlugin, _>(IpfsIoPlugin {
                    starting_realm: Some(final_config.server.clone()),
                    assets_root: Default::default(),
                })
                .add_before::<IpfsIoPlugin, _>(NftReaderPlugin),
        );

    if final_config.graphics.log_fps {
        app.add_plugins(FrameTimeDiagnosticsPlugin)
            .add_plugins(LogDiagnosticsPlugin::default());
    }

    app.insert_resource(SceneLoadDistance {
        load: final_config.scene_load_distance,
        unload: final_config.scene_unload_extra_distance,
    });

    app.insert_resource(final_config);
    if no_gltf {
        app.world.insert_resource(NoGltf(true));
    }

    app.configure_sets(Startup, SetupSets::Init.before(SetupSets::Main));

    app.add_plugins(UtilsPlugin)
        .add_plugins(InputManagerPlugin)
        .add_plugins(SceneRunnerPlugin)
        .add_plugins(UserInputPlugin)
        .add_plugins(UiCorePlugin)
        .add_plugins(SystemUiPlugin)
        .add_plugins(ConsolePlugin { add_egui: true })
        .add_plugins(VisualsPlugin { no_fog })
        .add_plugins(WalletPlugin)
        .add_plugins(CommsPlugin)
        .add_plugins(NftShapePlugin)
        .add_plugins(TweenPlugin)
        .add_plugins(SceneBoundPlugin)
        .add_plugins(CollectiblesPlugin)
        .add_plugins(WorldUiPlugin);

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
        .insert_resource(AmbientLight {
            color: Color::rgb(0.85, 0.85, 1.0),
            brightness: 575.0,
        });

    app.add_console_command::<ChangeLocationCommand, _>(change_location);
    app.add_console_command::<SceneDistanceCommand, _>(scene_distance);
    app.add_console_command::<SceneThreadsCommand, _>(scene_threads);
    app.add_console_command::<FpsCommand, _>(set_fps);

    // replay any warnings
    for warning in warnings {
        warn!(warning);
    }

    // requires local version of `bevy_mod_debugdump` due to once_cell version conflict.
    // probably resolved by updating deno. TODO: add feature flag for this after bumping deno
    // bevy_mod_debugdump::print_main_schedule(&mut app);

    app.run()
}

fn setup(
    mut commands: Commands,
    mut player_resource: ResMut<PrimaryPlayerRes>,
    mut cam_resource: ResMut<PrimaryCameraRes>,
    config: Res<AppConfig>,
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
            PrimaryUser::default(),
            OutOfWorld,
            AvatarDynamicState::default(),
            GroundCollider::default(),
        ))
        .push_children(&attach_points.entities())
        .insert(attach_points)
        .id();

    // add a camera
    let camera_id = commands
        .spawn((
            Camera3dBundle {
                camera: Camera {
                    hdr: true,
                    ..Default::default()
                },
                tonemapping: Tonemapping::TonyMcMapface,
                dither: DebandDither::Enabled,
                color_grading: ColorGrading {
                    exposure: -0.5,
                    gamma: 1.5,
                    pre_saturation: 1.0,
                    post_saturation: 1.0,
                },
                ..Default::default()
            },
            BloomSettings {
                intensity: 0.15,
                ..BloomSettings::OLD_SCHOOL
            },
            ShadowFilteringMethod::Castano13,
            PrimaryCamera::default(),
        ))
        .id();

    player_resource.0 = player_id;
    cam_resource.0 = camera_id;

    // add a directional light so it looks nicer
    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
            color: Color::rgb(1.0, 1.0, 0.7),
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
    });
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
