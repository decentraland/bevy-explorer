mod commands;
mod ext;
#[cfg(target_arch = "wasm32")]
mod web;
// POC: react-web HUD via CEF offscreen rendering into an in-engine texture (`react-hud-cef`).
#[cfg(all(not(target_arch = "wasm32"), feature = "react-hud-cef"))]
mod react_hud_cef;

#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;

use analytics::{metrics::MetricsPlugin, segment_system::SegmentConfig};
use assets::EmbedAssetsPlugin;
use av::AVPlayerPlugin;
use avatar::AvatarPlugin;
#[cfg(all(feature = "remote", not(target_arch = "wasm32")))]
use bevy::remote::{http::RemoteHttpPlugin, RemotePlugin};
#[cfg(not(target_arch = "wasm32"))]
use bevy::{
    app::TaskPoolThreadAssignmentPolicy,
    window::{PresentMode, WindowResolution},
};
use bevy::{
    app::{PluginGroupBuilder, Propagate},
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    log::LogPlugin,
    prelude::*,
    render::view::RenderLayers,
};
#[cfg(target_arch = "wasm32")]
use bevy::{
    asset::WasmLoaderHandle,
    render::{render_resource::PipelineCompilationMode, renderer::RenderDevice, RenderPlugin},
};
#[cfg(not(debug_assertions))]
use build_time::build_time_utc;
use collectibles::CollectiblesPlugin;
use common::{
    inputs::InputMap,
    sets::SetupSets,
    structs::{
        AppConfig, AvatarDynamicState, HeadSync, PointAtSync, PreviewMode, PrimaryCamera,
        PrimaryCameraRes, PrimaryPlayerRes, SceneImposterBake, SceneLoadDistance, StartupScene,
        StartupScenes, Version, GROUND_RENDERLAYER,
    },
    util::UtilsPlugin,
};
use comms::CommsPlugin;
use console::{ConsolePlugin, DoAddConsoleCommand};
use image_processing::ImageProcessingPlugin;
use imposters::DclImposterPlugin;
use input_manager::InputManagerPlugin;
use ipfs::{map_realm_name, IpfsIoPlugin};
use nft::{asset_source::NftReaderPlugin, NftShapePlugin};
use platform::default_camera_components;
use restricted_actions::process_startup_scenes;
use restricted_actions::RestrictedActionsPlugin;
use scene_inspector::SceneInspectorPlugin;
use scene_material::SceneBoundPlugin;
use scene_runner::{
    automatic_testing::AutomaticTestingPlugin,
    initialize_scene::{TestScenes, TestingData, PARCEL_SIZE},
    update_world::NoGltf,
    OutOfWorld, SceneRunnerPlugin,
};
use social::SocialPlugin;
use system_bridge::{settings::NewCameraEvent, NativeUi, SceneParams, SystemBridgePlugin};
#[cfg(not(target_arch = "wasm32"))]
use system_ui::crash_report::CrashReportPlugin;
use system_ui::SystemUiPlugin;
use texture_camera::TextureCameraPlugin;
use tween::TweenPlugin;
use ui_core::UiCorePlugin;
use user_input::{avatar_movement::GroundCollider, UserInputPlugin};
use uuid::Uuid;
use visuals::VisualsPlugin;
use wallet::WalletPlugin;
use world_ui::WorldUiPlugin;

#[cfg(target_arch = "wasm32")]
pub use crate::web::*;
use crate::{
    commands::{
        change_location, lock_preview, scene_distance, scene_threads, set_fps, unlock_preview,
        ChangeLocationCommand, FpsCommand, LockPreviewCommand, SceneDistanceCommand,
        SceneThreadsCommand, UnlockPreviewCommand,
    },
    ext::ReplaceIfSome,
};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
#[cfg(not(target_arch = "wasm32"))]
pub const DISTRIBUTION: &str = "desktop";
#[cfg(target_arch = "wasm32")]
pub const DISTRIBUTION: &str = "web";

pub struct DecentralandApp(App);

pub struct DecentralandAppConfig {
    pub app_config: AppConfig,
    pub arguments: DecentralandArguments,
    #[cfg(not(target_arch = "wasm32"))]
    pub crash_file: Option<PathBuf>,
    #[cfg(target_arch = "wasm32")]
    pub wasm_loader_handle: Option<WasmLoaderHandle>,
}

impl DecentralandAppConfig {
    pub fn new(
        mut app_config: AppConfig,
        arguments: DecentralandArguments,
        #[cfg(not(target_arch = "wasm32"))] crash_file: Option<PathBuf>,
        #[cfg(target_arch = "wasm32")] wasm_loader_handle: Option<WasmLoaderHandle>,
    ) -> Self {
        update_app_config_from_arguments(&mut app_config, &arguments);

        Self {
            app_config,
            arguments,
            #[cfg(not(target_arch = "wasm32"))]
            crash_file,
            #[cfg(target_arch = "wasm32")]
            wasm_loader_handle,
        }
    }

    /// The realm the engine boots into: an explicit --server, else the configured server.
    /// --server is deliberately NOT merged into the AppConfig: the config file is rewritten
    /// wholesale on any settings change, which would silently persist a one-off CLI realm
    /// as the configured (home) server.
    pub fn boot_server(&self) -> &str {
        self.arguments
            .server
            .as_deref()
            .unwrap_or(&self.app_config.server)
    }
}

pub struct DecentralandArguments {
    pub server: Option<String>,
    pub content_server_override: Option<String>,
    pub location: Option<IVec2>,
    pub startup_scenes: Option<Vec<StartupScene>>,
    pub ui_scene: Option<String>,
    pub scene_params: Option<String>,
    pub scene_threads: Option<usize>,
    pub scene_load_distance: Option<f32>,
    pub scene_unload_extra_distance: Option<f32>,
    pub scene_imposter_bake: Option<SceneImposterBake>,
    pub scene_imposter_distances: Option<Vec<f32>>,
    pub scene_imposter_multisample: Option<bool>,
    pub vsync: Option<bool>,
    pub fps_target: Option<usize>,
    pub gpu_bytes_per_frame: Option<usize>,
    pub is_preview: bool,
    pub sysinfo_visible: bool,
    pub scene_log_to_console: bool,
    pub startup_scenes_preview: bool,
    pub no_avatar: bool,
    pub no_gltf: bool,
    pub no_fog: bool,
    pub log_fps: Option<bool>,
    pub inspect: Option<String>,
    pub test_mode: bool,
    pub test_scenes: Option<TestScenes>,
    pub login: bool,
    pub emote_wheel: bool,
    pub chat: bool,
    pub permissions: bool,
    pub profile: bool,
    pub nametags: bool,
    pub tooltips: bool,
    pub loading_scene: bool,
}

impl DecentralandApp {
    /// Creates an [`App`] with [`LogPlugin`] so that logs
    /// work from the start
    pub fn new(log_plugin: LogPlugin) -> Self {
        let mut app = App::new();

        app.add_plugins(log_plugin);

        Self(app)
    }

    pub fn build(self, decentraland_app_config: DecentralandAppConfig) -> App {
        let mut app = self.0;

        // DefaultPlugins
        #[cfg(not(target_arch = "wasm32"))]
        app.add_plugins(desktop_default_plugins(&decentraland_app_config));
        #[cfg(target_arch = "wasm32")]
        app.add_plugins(wasm_default_plugins(&decentraland_app_config));

        // POC: react-web HUD composited in-engine from CEF offscreen rendering. Skipped in test
        // mode (automated scene tests run headless and must not boot CEF or gate input).
        #[cfg(all(not(target_arch = "wasm32"), feature = "react-hud-cef"))]
        if !decentraland_app_config.arguments.test_mode {
            app.add_plugins(react_hud_cef::ReactHudCefPlugin {
                // a non-default boot server (explicit --server or a configured home realm)
                // IS the destination: injected into the page URL as ?realm= so the HUD skips
                // its places picker (parity with ?realm= on web). On the stock default the
                // param is omitted so the picker shows — and the HUD's own default-realm
                // assumption then matches the realm the engine actually booted.
                server: (decentraland_app_config.boot_server() != AppConfig::default().server)
                    .then(|| decentraland_app_config.boot_server().to_owned()),
            });
        }

        let version_hash = version();
        let version = format!("{VERSION} ({version_hash})");

        info!("Bevy-Explorer version {}", version);

        let boot_server = map_realm_name(decentraland_app_config.boot_server());

        // Resources
        app.insert_resource(Version(version))
            .insert_resource(TestingData {
                inspect_hash: decentraland_app_config.arguments.inspect,
                test_mode: decentraland_app_config.arguments.test_mode,
                test_scenes: decentraland_app_config.arguments.test_scenes.clone(),
            })
            .insert_resource(PrimaryPlayerRes(Entity::PLACEHOLDER))
            .insert_resource(PrimaryCameraRes(Entity::PLACEHOLDER))
            .insert_resource(AmbientLight {
                color: Color::srgb(0.85, 0.85, 1.0),
                brightness: 575.0,
                ..Default::default()
            })
            .insert_resource(InputMap {
                inputs: decentraland_app_config
                    .app_config
                    .inputs
                    .0
                    .clone()
                    .into_iter()
                    .collect(),
                sensitivities: decentraland_app_config
                    .app_config
                    .inputs
                    .1
                    .clone()
                    .into_iter()
                    .collect(),
            })
            .insert_resource(PreviewMode {
                server: decentraland_app_config
                    .arguments
                    .is_preview
                    .then_some(boot_server),
                is_preview: decentraland_app_config.arguments.is_preview,
                preview_parcel: None,
            })
            .insert_resource(SceneLoadDistance {
                load: if decentraland_app_config.arguments.is_preview {
                    1.0
                } else {
                    decentraland_app_config.app_config.scene_load_distance
                },
                unload: if decentraland_app_config.arguments.is_preview {
                    0.0
                } else {
                    decentraland_app_config
                        .app_config
                        .scene_unload_extra_distance
                },
                load_imposter: decentraland_app_config
                    .app_config
                    .scene_imposter_distances
                    .last()
                    .map(|last| {
                        // actual distance we need is last + diagonal of the largest mip size
                        let mip_size = (1
                            << (decentraland_app_config
                                .app_config
                                .scene_imposter_distances
                                .len()
                                - 1)) as f32
                            * 16.0;
                        last + (2.0 * mip_size * mip_size).sqrt()
                    })
                    .unwrap_or(0.0)
                    * if decentraland_app_config.arguments.is_preview {
                        0.0
                    } else {
                        1.0
                    },
            })
            .insert_resource(SegmentConfig::new(
                decentraland_app_config.app_config.user_id.clone(),
                Uuid::new_v4().to_string(),
                version_hash,
            ));

        if decentraland_app_config.arguments.no_gltf {
            app.insert_resource(NoGltf(true));
        }

        // Purple background matching loading_background.png to avoid white flash on startup
        #[cfg(not(target_arch = "wasm32"))]
        app.insert_resource(ClearColor(Color::srgb(0.6, 0.1, 0.8)));

        let mut startup_scenes = decentraland_app_config
            .arguments
            .startup_scenes
            .unwrap_or_else(|| {
                vec![StartupScene {
                    source: String::from("basiccontroller.dcl.eth"),
                    super_user: false,
                    preview: decentraland_app_config.arguments.startup_scenes_preview,
                    hot_reload: None,
                    hash: None,
                }]
            });

        if let Some(source) = decentraland_app_config.arguments.ui_scene {
            app.insert_resource(NativeUi {
                login: decentraland_app_config.arguments.login,
                emote_wheel: decentraland_app_config.arguments.emote_wheel,
                chat: decentraland_app_config.arguments.chat,
                permissions: decentraland_app_config.arguments.permissions,
                profile: decentraland_app_config.arguments.profile,
                nametags: decentraland_app_config.arguments.nametags,
                tooltips: decentraland_app_config.arguments.tooltips,
                loading_scene: decentraland_app_config.arguments.loading_scene,
            });
            startup_scenes.insert(
                0,
                StartupScene {
                    source,
                    super_user: true,
                    preview: decentraland_app_config.arguments.startup_scenes_preview,
                    hot_reload: None,
                    hash: None,
                },
            );
        } else {
            app.insert_resource(NativeUi {
                login: true,
                emote_wheel: true,
                chat: true,
                permissions: true,
                profile: true,
                nametags: true,
                tooltips: true,
                loading_scene: true,
            });
        }

        // POC: the react-web overlay is the HUD — turn off the engine's native UI so it doesn't
        // render its own login/chat/etc. behind the webview. (Overrides the inserts above.)
        // Test mode keeps the native UI: the HUD plugin is skipped there.
        #[cfg(all(not(target_arch = "wasm32"), feature = "react-hud-cef"))]
        if !decentraland_app_config.arguments.test_mode {
            app.insert_resource(NativeUi {
                login: false,
                emote_wheel: false,
                chat: false,
                permissions: false,
                profile: false,
                nametags: false,
                tooltips: false,
                loading_scene: false,
            });
        }

        if !startup_scenes.is_empty() {
            app.add_systems(Update, process_startup_scenes);
            info!("spawning {} startup scenes", startup_scenes.len());
            app.insert_resource(StartupScenes {
                scenes: startup_scenes,
            });
        }

        app.insert_resource(SceneParams::from_query_string(
            &decentraland_app_config
                .arguments
                .scene_params
                .unwrap_or_default(),
            cfg!(target_arch = "wasm32"),
        ));

        // Create copies of structs that still need to be accessed
        // and add AppConfig as a resource
        let graphics_config = decentraland_app_config.app_config.graphics.clone();
        app.insert_resource(decentraland_app_config.app_config.audio.clone());
        app.insert_resource(decentraland_app_config.app_config);

        // Plugins
        app.add_plugins(SceneRunnerPlugin)
            .add_plugins(AVPlayerPlugin)
            .add_plugins(RestrictedActionsPlugin)
            .add_plugins(UtilsPlugin)
            .add_plugins(InputManagerPlugin)
            .add_plugins(SceneBoundPlugin)
            .add_plugins(UserInputPlugin)
            .add_plugins(UiCorePlugin)
            .add_plugins(SystemUiPlugin)
            .add_plugins(ConsolePlugin {
                add_bevy_console: true,
            })
            .add_plugins(VisualsPlugin {
                no_fog: decentraland_app_config.arguments.no_fog,
            })
            .add_plugins(WalletPlugin)
            .add_plugins(CommsPlugin)
            .add_plugins(SocialPlugin)
            .add_plugins(NftShapePlugin)
            .add_plugins(TweenPlugin)
            .add_plugins(CollectiblesPlugin)
            .add_plugins(WorldUiPlugin)
            .add_plugins(TextureCameraPlugin)
            .add_plugins(ImageProcessingPlugin)
            .add_plugins(SystemBridgePlugin { bare: false })
            .add_plugins(SceneInspectorPlugin)
            .add_plugins(EmbedAssetsPlugin);

        if !decentraland_app_config.arguments.is_preview {
            app.add_plugins(DclImposterPlugin {
                zip_output: None,
                download: true,
            });
        }
        if !decentraland_app_config.arguments.no_avatar {
            app.add_plugins(AvatarPlugin);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(crashed) = decentraland_app_config.crash_file {
            if let Ok(file) = std::path::Path::canonicalize(&crashed) {
                app.add_plugins(CrashReportPlugin { file });
            }
        }

        #[cfg(all(feature = "remote", not(target_arch = "wasm32")))]
        app.add_plugins((RemotePlugin::default(), RemoteHttpPlugin::default()));
        #[cfg(feature = "bevy_mesh_picking_backend")]
        app.add_plugins(MeshPickingPlugin);

        // Analytics plugins
        app.add_plugins(MetricsPlugin);
        if (graphics_config.log_fps || decentraland_app_config.arguments.is_preview)
            && !app.is_plugin_added::<FrameTimeDiagnosticsPlugin>()
        {
            app.add_plugins(FrameTimeDiagnosticsPlugin::default());
        }
        if graphics_config.log_fps {
            app.add_plugins(LogDiagnosticsPlugin::default());
        }

        if decentraland_app_config.arguments.test_scenes.is_some() {
            app.add_plugins(AutomaticTestingPlugin);
        }

        // Systems
        app.configure_sets(Startup, SetupSets::Init.before(SetupSets::Main));
        app.add_systems(Startup, setup.in_set(SetupSets::Init));

        // Commands
        app.add_console_command::<ChangeLocationCommand, _>(change_location);
        app.add_console_command::<SceneDistanceCommand, _>(scene_distance);
        app.add_console_command::<LockPreviewCommand, _>(lock_preview);
        app.add_console_command::<UnlockPreviewCommand, _>(unlock_preview);
        app.add_console_command::<SceneThreadsCommand, _>(scene_threads);
        app.add_console_command::<FpsCommand, _>(set_fps);

        app
    }
}

fn setup(
    mut commands: Commands,
    mut player_resource: ResMut<PrimaryPlayerRes>,
    mut cam_resource: ResMut<PrimaryCameraRes>,
    config: Res<AppConfig>,
    #[cfg(target_arch = "wasm32")] render_device: ResMut<RenderDevice>,
) {
    #[cfg(target_arch = "wasm32")]
    render_device
        .wgpu_device()
        .on_uncaptured_error(Box::new(|e: wgpu::Error| {
            error!("captured wgpu error: {e:?}")
        }));

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
            HeadSync::default(),
            PointAtSync::default(),
            GroundCollider::default(),
            Propagate(RenderLayers::default()),
        ))
        .id();

    // add a camera
    let camera_id = commands
        .spawn((
            Camera3d::default(),
            Camera {
                hdr: true,
                ..Default::default()
            },
            default_camera_components(),
            Projection::from(PerspectiveProjection {
                far: 100000.0,
                ..Default::default()
            }),
            PrimaryCamera::default(),
            GROUND_RENDERLAYER.with(0),
        ))
        .id();
    commands.send_event(NewCameraEvent(camera_id));
    player_resource.0 = player_id;
    cam_resource.0 = camera_id;
}

fn update_app_config_from_arguments(
    base_app_config: &mut AppConfig,
    arguments: &DecentralandArguments,
) {
    base_app_config.location.replace_if_some(arguments.location);

    base_app_config
        .graphics
        .vsync
        .replace_if_some(arguments.vsync);
    base_app_config
        .graphics
        .log_fps
        .replace_if_some(arguments.log_fps);
    base_app_config
        .graphics
        .fps_target
        .replace_if_some(arguments.fps_target);
    base_app_config
        .graphics
        .gpu_bytes_per_frame
        .replace_if_some(arguments.gpu_bytes_per_frame);

    base_app_config
        .scene_threads
        .replace_if_some(arguments.scene_threads);
    base_app_config
        .scene_load_distance
        .replace_if_some(arguments.scene_load_distance);
    base_app_config
        .scene_unload_extra_distance
        .replace_if_some(arguments.scene_unload_extra_distance);
    base_app_config
        .scene_imposter_bake
        .replace_if_some(arguments.scene_imposter_bake);

    base_app_config
        .scene_imposter_distances
        .replace_if_some(arguments.scene_imposter_distances.clone());
    base_app_config.scene_imposter_distances = base_app_config
        .scene_imposter_distances
        .iter()
        .enumerate()
        .map(|(ix, d)| {
            let edge_distance = (1 << ix) as f32 * PARCEL_SIZE;
            let diagonal_distance = (edge_distance * edge_distance * 2.0).sqrt();
            // println!("[{ix}] -> {}", d.max(diagonal_distance));
            d.max(diagonal_distance)
        })
        .collect();

    base_app_config
        .scene_imposter_multisample
        .replace_if_some(arguments.scene_imposter_multisample);
    base_app_config.sysinfo_visible |= arguments.sysinfo_visible;
    base_app_config.scene_log_to_console |= arguments.scene_log_to_console;
}

#[cfg(not(target_arch = "wasm32"))]
fn desktop_default_plugins(decentraland_app_config: &DecentralandAppConfig) -> PluginGroupBuilder {
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
            primary_window: Some(Window {
                title: "Decentraland Web Explorer".to_owned(),
                present_mode: if decentraland_app_config.app_config.graphics.vsync {
                    PresentMode::AutoVsync
                } else {
                    PresentMode::AutoNoVsync
                },
                resolution: WindowResolution::new(1280.0, 720.0),
                ..Default::default()
            }),
            ..Default::default()
        })
        .disable::<LogPlugin>()
        .set(bevy::asset::AssetPlugin {
            // we manage asset server loads via ipfs module, so we don't need this protection
            unapproved_path_mode: bevy::asset::UnapprovedPathMode::Allow,
            ..Default::default()
        })
        .build()
        .add_before::<bevy::asset::AssetPlugin>(IpfsIoPlugin {
            preview: decentraland_app_config.arguments.is_preview,
            starting_realm: Some(map_realm_name(decentraland_app_config.boot_server())),
            content_server_override: decentraland_app_config
                .arguments
                .content_server_override
                .clone(),
            assets_root: Default::default(),
            num_slots: decentraland_app_config.app_config.max_concurrent_remotes,
        })
        .add_before::<IpfsIoPlugin>(NftReaderPlugin)
}

#[cfg(target_arch = "wasm32")]
fn wasm_default_plugins(decentraland_app_config: &DecentralandAppConfig) -> PluginGroupBuilder {
    DefaultPlugins
        .set(RenderPlugin {
            pipeline_compilation_mode: PipelineCompilationMode::async_with_handler(PipelineHandler),
            ..default()
        })
        .set(WindowPlugin {
            primary_window: Some(Window {
                canvas: Some("#mygame-canvas".into()),
                fit_canvas_to_parent: true,
                ..default()
            }),
            ..Default::default()
        })
        .set(AssetPlugin {
            // we manage asset server loads via ipfs module, so we don't need this protection
            wasm_loader_handle: decentraland_app_config.wasm_loader_handle.clone(),
            unapproved_path_mode: bevy::asset::UnapprovedPathMode::Allow,
            ..Default::default()
        })
        .disable::<LogPlugin>()
        .add_before::<AssetPlugin>(IpfsIoPlugin {
            preview: decentraland_app_config.arguments.is_preview,
            starting_realm: Some(map_realm_name(decentraland_app_config.boot_server())),
            content_server_override: decentraland_app_config
                .arguments
                .content_server_override
                .clone(),
            assets_root: Default::default(),
            num_slots: decentraland_app_config.app_config.max_concurrent_remotes,
        })
        .add_before::<IpfsIoPlugin>(NftReaderPlugin)
}

pub fn version() -> String {
    #[cfg(not(debug_assertions))]
    return format!(
        "bevy-{}-{DISTRIBUTION}-{}{}",
        std::env::consts::OS,
        env!("BEVY_EXPLORER_VERSION"),
        (env!("BEVY_EXPLORER_LOCAL_MODIFICATION") == "true")
            .then_some(format!("-{}", build_time_utc!("%Y-%m-%d %H:%M")))
            .unwrap_or_default()
    );

    #[cfg(debug_assertions)]
    "debug".to_string()
}
