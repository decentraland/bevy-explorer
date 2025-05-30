use analytics::segment_system::SegmentConfig;
use bevy::{
    core_pipeline::{
        bloom::BloomSettings,
        tonemapping::{DebandDither, Tonemapping},
    },
    pbr::ShadowFilteringMethod,
    prelude::*,
    render::view::{ColorGrading, ColorGradingGlobal, ColorGradingSection, RenderLayers},
    tasks::{IoTaskPool, Task},
};
use bevy_console::ConsoleCommand;
use imposters::DclImposterPlugin;
use std::str::FromStr;

use collectibles::CollectiblesPlugin;
use common::{
    inputs::InputMap,
    sets::SetupSets,
    structs::{
        AppConfig, AttachPoints, AvatarDynamicState, IVec2Arg, PreviewCommand, PrimaryCamera,
        PrimaryCameraRes, PrimaryPlayerRes, PrimaryUser, SceneLoadDistance, SystemScene, Version,
        GROUND_RENDERLAYER,
    },
    util::{TaskCompat, TaskExt, TryPushChildrenEx, UtilsPlugin},
};
use restricted_actions::{lookup_portable, RestrictedActionsPlugin};
use scene_material::SceneBoundPlugin;
use scene_runner::{
    initialize_scene::{PortableScenes, PortableSource, TestingData},
    update_world::mesh_collider::GroundCollider,
    OutOfWorld, SceneRunnerPlugin,
};

use av::AudioPlugin;
use avatar::AvatarPlugin;
use comms::{
    preview::{handle_preview_socket, PreviewMode},
    CommsPlugin,
};
use console::{ConsolePlugin, DoAddConsoleCommand};
use input_manager::InputManagerPlugin;
use ipfs::{IpfsAssetServer, IpfsIoPlugin};
use nft::{asset_source::NftReaderPlugin, NftShapePlugin};
use platform::{DepthPrepass, NormalPrepass};
use social::SocialPlugin;
use system_bridge::{NativeUi, SystemBridgePlugin};
use system_ui::SystemUiPlugin;
use texture_camera::TextureCameraPlugin;
use tween::TweenPlugin;
use ui_core::UiCorePlugin;
use user_input::UserInputPlugin;
use uuid::Uuid;
use visuals::VisualsPlugin;
use wallet::WalletPlugin;
use world_ui::WorldUiPlugin;

fn main_inner(server: &str, location: &str) {
    // warnings before log init must be stored and replayed later
    let mut app = App::new();

    let final_config = AppConfig {
        server: server.to_owned(),
        location: IVec2Arg::from_str(location)
            .map(|l| l.0)
            .unwrap_or(IVec2::ZERO),
        ..Default::default()
    };

    let content_server_override = None;
    let test_scenes = None;
    let test_mode = false;

    app.insert_resource(TestingData {
        inspect_hash: None,
        test_mode,
        test_scenes: test_scenes.clone(),
    });

    let no_fog = false;
    let is_preview = false;

    app.insert_resource(NativeUi {
        login: true,
        emote_wheel: true,
        chat: true,
    });

    let version = format!("webgl proof of concept");

    app.insert_resource(Version(version.clone()))
        .insert_resource(final_config.audio.clone())
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        // provide the ID selector string here
                        canvas: Some("#mygame-canvas".into()),
                        // ... any other window properties ...
                        ..default()
                    }),
                    ..Default::default()
                })
                .build()
                .add_before::<bevy::asset::AssetPlugin, _>(IpfsIoPlugin {
                    preview: is_preview,
                    starting_realm: Some(final_config.server.clone()),
                    content_server_override,
                    assets_root: Default::default(),
                    num_slots: final_config.max_concurrent_remotes,
                })
                .add_before::<IpfsIoPlugin, _>(NftReaderPlugin),
        );

    app.insert_resource(InputMap {
        inputs: final_config.inputs.0.clone().into_iter().collect(),
    });

    app.insert_resource(SegmentConfig::new(
        final_config.user_id.clone(),
        Uuid::new_v4().to_string(),
        "web test".to_owned(),
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
                last + (2.0 * mip_size * mip_size).sqrt()
            })
            .unwrap_or(0.0),
    });

    app.insert_resource(final_config);
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
        .add_plugins(DclImposterPlugin {
            zip_output: None,
            download: true,
        })
        .add_plugins(TextureCameraPlugin)
        .add_plugins(SystemBridgePlugin { bare: false });

    app.add_plugins(AvatarPlugin);

    app.add_plugins(AudioPlugin)
        .add_plugins(RestrictedActionsPlugin)
        .insert_resource(PrimaryPlayerRes(Entity::PLACEHOLDER))
        .insert_resource(PrimaryCameraRes(Entity::PLACEHOLDER))
        .add_systems(Startup, setup.in_set(SetupSets::Init))
        .insert_resource(AmbientLight {
            color: Color::srgb(0.85, 0.85, 1.0),
            brightness: 575.0,
        });

    app.add_console_command::<ChangeLocationCommand, _>(change_location);
    app.add_console_command::<SceneDistanceCommand, _>(scene_distance);
    app.add_console_command::<SceneThreadsCommand, _>(scene_threads);
    app.add_console_command::<FpsCommand, _>(set_fps);

    info!("Bevy-Explorer version {}", version);

    app.run();
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
            config.player_settings.clone(),
            OutOfWorld,
            AvatarDynamicState::default(),
            GroundCollider::default(),
            propagate::Propagate(RenderLayers::default()),
        ))
        .try_push_children(&attach_points.entities())
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
            GROUND_RENDERLAYER.with(0),
        ))
        .id();

    player_resource.0 = player_id;
    cam_resource.0 = camera_id;
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

#[allow(clippy::type_complexity)]
pub fn process_system_ui_scene(
    mut system_scene: ResMut<SystemScene>,
    mut task: Local<Option<Task<Result<(String, PortableSource), String>>>>,
    mut done: Local<bool>,
    mut portables: ResMut<PortableScenes>,
    ipfas: IpfsAssetServer,
    mut channel: Local<Option<tokio::sync::mpsc::UnboundedReceiver<PreviewCommand>>>,
    mut writer: EventWriter<PreviewCommand>,
) {
    if let Some(command) = channel.as_mut().and_then(|rx| rx.try_recv().ok()) {
        writer.send(command);
        *done = false;
        system_scene.hash = None;
        return;
    }

    if *done || system_scene.source.is_none() {
        return;
    }

    if task.is_none() {
        *task = Some(IoTaskPool::get().spawn_compat(lookup_portable(
            None,
            system_scene.source.clone().unwrap(),
            true,
            ipfas.ipfs().clone(),
        )));
    }

    let mut t = task.take().unwrap();
    match t.complete() {
        Some(Ok((hash, source))) => {
            info!("added ui scene from {}", source.pid);
            system_scene.hash = Some(hash.clone());
            portables.0.extend([(hash, source)]);
            *done = true;

            if system_scene.preview {
                let (sx, rx) = tokio::sync::mpsc::unbounded_channel();
                IoTaskPool::get()
                    .spawn(handle_preview_socket(
                        system_scene.source.clone().unwrap(),
                        sx.clone(),
                    ))
                    .detach();
                *channel = Some(rx);
                system_scene.hot_reload = Some(sx);
            }
        }
        Some(Err(e)) => {
            error!("failed to load ui scene: {e}");
            *done = true;
        }
        None => *task = Some(t),
    }
}

use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn initialize() -> Result<(), JsValue> {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    console_log::init_with_level(log::Level::Info).expect("error initializing logger");
    Ok(())
}

#[wasm_bindgen]
pub fn wasm_run(realm: &str, location: &str) {
    main_inner(realm, location);
}
