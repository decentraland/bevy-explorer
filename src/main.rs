// todo
// - separate js crate
// - budget -> deadline is just last end + frame time

use avatar::AvatarDynamicState;
use bevy::{
    core_pipeline::tonemapping::{DebandDither, Tonemapping},
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    pbr::CascadeShadowConfigBuilder,
    prelude::*,
    render::view::ColorGrading,
};
use bevy_console::ConsoleCommand;
use bevy_prototype_debug_lines::DebugLinesPlugin;

use common::{
    sets::SetupSets,
    structs::{AppConfig, GraphicsSettings, PrimaryCamera, PrimaryCameraRes, PrimaryUser},
};
use scene_runner::{
    initialize_scene::SceneLoadDistance, update_world::mesh_collider::GroundCollider,
    SceneRunnerPlugin,
};

use avatar::AvatarPlugin;
use comms::{wallet::WalletPlugin, CommsPlugin};
use console::{ConsolePlugin, DoAddConsoleCommand};
use input_manager::InputManagerPlugin;
use ipfs::IpfsIoPlugin;
use system_ui::SystemUiPlugin;
use ui_core::UiCorePlugin;
use user_input::UserInputPlugin;
use visuals::VisualsPlugin;

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
        },
        scene_threads: args
            .value_from_str("--threads")
            .ok()
            .unwrap_or(base_config.scene_threads),
        scene_loop_millis: args
            .value_from_str("--millis")
            .ok()
            .unwrap_or(base_config.scene_loop_millis),
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
                filter: "wgpu=error,bevy_animation=error".to_string(),
                ..default()
            })
            .build()
            .add_before::<bevy::asset::AssetPlugin, _>(IpfsIoPlugin {
                starting_realm: Some(final_config.server.clone()),
                cache_root: Default::default(),
            }),
    );

    if final_config.graphics.log_fps {
        app.add_plugin(FrameTimeDiagnosticsPlugin)
            .add_plugin(LogDiagnosticsPlugin::default());
    }

    app.insert_resource(final_config);

    app.configure_set(SetupSets::Init.before(SetupSets::Main));

    app.add_plugin(DebugLinesPlugin::with_depth_test(true))
        .add_plugin(bevy_mod_billboard::prelude::BillboardPlugin)
        .add_plugin(InputManagerPlugin)
        .add_plugin(SceneRunnerPlugin)
        .add_plugin(UserInputPlugin)
        .add_plugin(UiCorePlugin)
        .add_plugin(SystemUiPlugin)
        .add_plugin(ConsolePlugin { add_egui: true })
        .add_plugin(VisualsPlugin)
        .add_plugin(WalletPlugin)
        .add_plugin(CommsPlugin)
        .add_plugin(AvatarPlugin)
        .insert_resource(PrimaryCameraRes(Entity::PLACEHOLDER))
        .add_startup_system(setup.in_set(SetupSets::Init))
        .insert_resource(AmbientLight {
            color: Color::rgb(0.85, 0.85, 1.0),
            brightness: 0.5,
        });

    app.add_console_command::<ChangeLocationCommand, _>(change_location);
    app.add_console_command::<SceneDistanceCommand, _>(scene_distance);
    app.add_console_command::<SceneThreadsCommand, _>(scene_threads);
    app.add_console_command::<SceneMillisCommand, _>(scene_millis);

    // replay any warnings
    for warning in warnings {
        warn!(warning);
    }

    // requires local version of `bevy_mod_debugdump` due to once_cell version conflict.
    // probably resolved by updating deno. TODO: add feature flag for this after bumping deno
    // bevy_mod_debugdump::print_main_schedule(&mut app);

    app.run()
}

fn setup(mut commands: Commands, mut cam_resource: ResMut<PrimaryCameraRes>) {
    info!("main::setup");
    // create the main player
    commands.spawn((
        SpatialBundle {
            transform: Transform::from_translation(Vec3::new(16.0 * 78.5, 0.0, 16.0 * 6.5)),
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
                    // TODO enable when we can use gizmos instead of debuglines in bevy 0.11
                    // hdr: true,
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
    x: i32,
    y: i32,
}

fn change_location(
    mut input: ConsoleCommand<ChangeLocationCommand>,
    mut player: Query<&mut Transform, With<PrimaryUser>>,
) {
    if let Some(Ok(command)) = input.take() {
        if let Ok(mut transform) = player.get_single_mut() {
            transform.translation.x = command.x as f32 * 16.0;
            transform.translation.z = command.y as f32 * 16.0;
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

// set loop millis
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/scene_millis")]
struct SceneMillisCommand {
    millis: Option<u64>,
}

fn scene_millis(mut input: ConsoleCommand<SceneMillisCommand>, mut config: ResMut<AppConfig>) {
    if let Some(Ok(command)) = input.take() {
        let millis = command.millis.unwrap_or(12);
        config.scene_loop_millis = millis;
        input.reply_ok("scene loop max ms set to {millis}");
    }
}
