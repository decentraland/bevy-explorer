// todo
// - separate js crate
// - budget -> deadline is just last end + frame time

use std::{num::ParseIntError, str::FromStr};

use avatar::AvatarDynamicState;
use bevy::{
    core_pipeline::{
        bloom::BloomSettings,
        tonemapping::{DebandDither, Tonemapping},
    },
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    pbr::CascadeShadowConfigBuilder,
    prelude::*,
    render::view::ColorGrading,
};
use bevy_console::ConsoleCommand;

use common::{
    sets::SetupSets,
    structs::{AppConfig, GraphicsSettings, PrimaryCamera, PrimaryCameraRes, PrimaryUser},
};
use scene_runner::{
    initialize_scene::SceneLoadDistance, update_world::mesh_collider::GroundCollider,
    SceneRunnerPlugin,
};

use av::AudioPlugin;
use avatar::AvatarPlugin;
use comms::{wallet::WalletPlugin, CommsPlugin};
use console::{ConsolePlugin, DoAddConsoleCommand};
use input_manager::InputManagerPlugin;
use ipfs::IpfsIoPlugin;
use system_ui::SystemUiPlugin;
use ui_core::UiCorePlugin;
use user_input::UserInputPlugin;
use visuals::VisualsPlugin;

#[derive(Debug)]
struct IVec2Arg(IVec2);

impl FromStr for IVec2Arg {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut chars = s.chars().peekable();

        let skip = |chars: &mut std::iter::Peekable<std::str::Chars>, numeric: bool| {
            while numeric
                == chars
                    .peek()
                    .map_or(!numeric, |c| c.is_numeric() || *c == '-')
            {
                chars.next();
            }
        };

        let parse = |chars: &std::iter::Peekable<std::str::Chars>| {
            chars
                .clone()
                .take_while(|c| c.is_numeric() || *c == '-')
                .collect::<String>()
                .parse::<i32>()
        };

        skip(&mut chars, false);
        let x = parse(&chars)?;
        skip(&mut chars, true);
        skip(&mut chars, false);
        let y = parse(&chars)?;

        Ok(IVec2Arg(IVec2::new(x, y)))
    }
}

fn main() {
    // warnings before log init must be stored and replayed later
    let mut warnings = Vec::default();

    let base_config: AppConfig = std::fs::read("config.json")
        .ok()
        .and_then(|f| {
            serde_json::from_slice(&f)
                .map_err(|e| warnings.push(format!("failed to parse config.json: {e}")))
                .ok()
        })
        .unwrap_or(Default::default());
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
        profile_version: base_config.profile_version,
        profile_content: base_config.profile_content,
        profile_base_url: base_config.profile_base_url,
        graphics: GraphicsSettings {
            vsync: args
                .value_from_str("--vsync")
                .ok()
                .unwrap_or(base_config.graphics.vsync),
            log_fps: args
                .value_from_str("--log_fps")
                .ok()
                .unwrap_or(base_config.graphics.log_fps),
            msaa: args
                .value_from_str::<_, usize>("--msaa")
                .ok()
                .unwrap_or(base_config.graphics.msaa),
            fps_target: args
                .value_from_str::<_, usize>("--fps")
                .ok()
                .unwrap_or(base_config.graphics.fps_target),
        },
        scene_threads: args
            .value_from_str("--threads")
            .ok()
            .unwrap_or(base_config.scene_threads),
    };

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

    let mut app = App::new();
    let present_mode = match final_config.graphics.vsync {
        true => bevy::window::PresentMode::AutoVsync,
        false => bevy::window::PresentMode::AutoNoVsync,
    };

    let msaa = match final_config.graphics.msaa {
        1 => Msaa::Off,
        2 => Msaa::Sample2,
        4 => Msaa::Sample4,
        8 => Msaa::Sample8,
        _ => {
            warnings.push(
                "Invalid msaa sample count, must be one of (1, 2, 4, 8). Defaulting to Off"
                    .to_owned(),
            );
            Msaa::Off
        }
    };

    app.insert_resource(msaa).add_plugins(
        DefaultPlugins
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
                cache_root: Default::default(),
            }),
    );

    if final_config.graphics.log_fps {
        app.add_plugins(FrameTimeDiagnosticsPlugin)
            .add_plugins(LogDiagnosticsPlugin::default());
    }

    app.insert_resource(final_config);

    app.configure_set(Startup, SetupSets::Init.before(SetupSets::Main));

    app.add_plugins(bevy_mod_billboard::prelude::BillboardPlugin)
        .add_plugins(InputManagerPlugin)
        .add_plugins(SceneRunnerPlugin)
        .add_plugins(UserInputPlugin)
        .add_plugins(UiCorePlugin)
        .add_plugins(SystemUiPlugin)
        .add_plugins(ConsolePlugin { add_egui: true })
        .add_plugins(VisualsPlugin)
        .add_plugins(WalletPlugin)
        .add_plugins(CommsPlugin)
        .add_plugins(AvatarPlugin)
        .add_plugins(AudioPlugin)
        .insert_resource(PrimaryCameraRes(Entity::PLACEHOLDER))
        .add_systems(Startup, setup.in_set(SetupSets::Init))
        .insert_resource(AmbientLight {
            color: Color::rgb(0.85, 0.85, 1.0),
            brightness: 0.5,
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
    mut cam_resource: ResMut<PrimaryCameraRes>,
    config: Res<AppConfig>,
) {
    info!("main::setup");
    // create the main player
    commands.spawn((
        SpatialBundle {
            transform: Transform::from_translation(Vec3::new(
                16.0 * config.location.x as f32,
                0.0,
                -16.0 * config.location.y as f32,
            )),
            ..Default::default()
        },
        PrimaryUser::default(),
        AvatarDynamicState::default(),
        GroundCollider::default(),
    ));

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
            PrimaryCamera::default(),
        ))
        .id();

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
    mut input: ConsoleCommand<ChangeLocationCommand>,
    mut player: Query<&mut Transform, With<PrimaryUser>>,
) {
    if let Some(Ok(command)) = input.take() {
        if let Ok(mut transform) = player.get_single_mut() {
            transform.translation.x = command.x as f32 * 16.0;
            transform.translation.z = -command.y as f32 * 16.0;
            input.reply_ok(format!("new location: {:?}", (command.x, command.y)));
            return;
        }

        input.reply_failed("failed to set location");
    }
}

/// set scene load distance (defaults to 100.0m)
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/scene_distance")]
struct SceneDistanceCommand {
    distance: Option<f32>,
}

fn scene_distance(
    mut input: ConsoleCommand<SceneDistanceCommand>,
    mut scene_load_distance: ResMut<SceneLoadDistance>,
) {
    if let Some(Ok(command)) = input.take() {
        let distance = command.distance.unwrap_or(100.0);
        scene_load_distance.0 = distance;
        input.reply_ok("set scene load distance to {distance}");
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
