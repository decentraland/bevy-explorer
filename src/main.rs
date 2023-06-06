// todo
// - separate js crate
// - budget -> deadline is just last end + frame time

pub mod avatar;
pub mod common;
pub mod comms;
pub mod console;
pub mod dcl;
pub mod dcl_component;
pub mod input_manager;
pub mod ipfs;
pub mod scene_runner;
pub mod system_ui;
pub mod user_input;
pub mod util;
pub mod visuals;

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

use common::{PrimaryCamera, PrimaryUser};
use comms::profile::UserProfile;
use scene_runner::{
    initialize_scene::SceneLoadDistance, update_world::mesh_collider::GroundCollider,
    SceneRunnerPlugin,
};
use serde::{Deserialize, Serialize};

use crate::{
    avatar::AvatarPlugin,
    comms::{wallet::WalletPlugin, CommsPlugin},
    console::{ConsolePlugin, DoAddConsoleCommand},
    input_manager::InputManagerPlugin,
    ipfs::IpfsIoPlugin,
    system_ui::SystemUiPlugin,
    user_input::UserInputPlugin,
    visuals::VisualsPlugin,
};

// macro for assertions
// by default, enabled in debug builds and disabled in release builds
// can be enabled for release with `cargo run --release --features="dcl-assert"`
#[cfg(any(debug_assertions, feature = "dcl-assert"))]
#[macro_export]
macro_rules! dcl_assert {
    ($($arg:tt)*) => ( assert!($($arg)*); )
}
#[cfg(not(any(debug_assertions, feature = "dcl-assert")))]
#[macro_export]
macro_rules! dcl_assert {
    ($($arg:tt)*) => {};
}

#[derive(Serialize, Deserialize)]
pub struct GraphicsSettings {
    vsync: bool,
    log_fps: bool,
    msaa: usize,
}

impl Default for GraphicsSettings {
    fn default() -> Self {
        Self {
            vsync: false,
            log_fps: true,
            msaa: 4,
        }
    }
}

#[derive(Serialize, Deserialize, Resource)]
pub struct AppConfig {
    pub server: String,
    pub profile: UserProfile,
    pub graphics: GraphicsSettings,
    pub scene_threads: usize,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: "https://sdk-team-cdn.decentraland.org/ipfs/goerli-plaza-main".to_owned(),
            profile: UserProfile {
                version: 1,
                content: Default::default(),
                base_url: "https://peer.decentraland.zone/content/contents/".to_owned(),
            },
            graphics: Default::default(),
            scene_threads: 4,
        }
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
        profile: base_config.profile,
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

    app.add_plugin(DebugLinesPlugin::with_depth_test(true))
        .add_plugin(bevy_mod_billboard::prelude::BillboardPlugin)
        .add_plugin(InputManagerPlugin)
        .add_plugin(SceneRunnerPlugin)
        .add_plugin(UserInputPlugin)
        .add_plugin(SystemUiPlugin)
        .add_plugin(ConsolePlugin)
        .add_plugin(VisualsPlugin)
        .add_plugin(WalletPlugin)
        .add_plugin(CommsPlugin)
        .add_plugin(AvatarPlugin)
        .add_startup_system(setup)
        .insert_resource(AmbientLight {
            color: Color::rgb(0.75, 0.75, 1.0),
            brightness: 0.25,
        });

    app.add_console_command::<ChangeLocationCommand, _>(change_location);
    app.add_console_command::<SceneDistanceCommand, _>(scene_distance);

    // replay any warnings
    for warning in warnings {
        warn!(warning);
    }

    // requires local version of `bevy_mod_debugdump` due to once_cell version conflict.
    // probably resolved by updating deno. TODO: add feature flag for this after bumping deno
    // bevy_mod_debugdump::print_main_schedule(&mut app);

    app.run()
}

fn setup(mut commands: Commands) {
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
    commands.spawn((
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
    ));

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

// hook console commands
#[cfg(not(test))]
impl console::DoAddConsoleCommand for App {
    fn add_console_command<T: bevy_console::Command, U>(
        &mut self,
        system: impl IntoSystemConfig<U>,
    ) -> &mut Self {
        bevy_console::AddConsoleCommand::add_console_command::<T, U>(self, system)
    }
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
        input.reply_failed("set scene load distance to {distance}");
    }
}
