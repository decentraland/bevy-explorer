mod commands;
mod ext;
#[cfg(target_arch = "wasm32")]
mod web;
// POC: react-web HUD via CEF offscreen rendering into an in-engine texture (`react-hud-cef`).
#[cfg(all(not(target_arch = "wasm32"), feature = "react-hud-cef"))]
mod react_hud_cef;

#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
use std::str::FromStr;

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
        AppConfig, AvatarDynamicState, EditorMode, HeadSync, IVec2Arg, PointAtSync, PreviewMode,
        PrimaryCamera, PrimaryCameraRes, PrimaryPlayerRes, SceneImposterBake, SceneLoadDistance,
        ShowOutOfBounds, StartupScene, StartupScenes, Version, GROUND_RENDERLAYER,
    },
    util::UtilsPlugin,
};
use comms::CommsPlugin;
use console::{ConsolePlugin, DoAddConsoleCommand};
use image_processing::ImageProcessingPlugin;
use imposters::DclImposterPlugin;
use input_manager::InputManagerPlugin;
use ipfs::{map_realm_name, IpfsIoPlugin};
use livestream_manager::plugin::LivestreamManagerPlugin;
use nft::{asset_source::NftReaderPlugin, NftShapePlugin};
use particle_system::plugin::ParticleSystemPlugin;
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
use system_api_types::{launch_options::LaunchOptions, web_params::DEFAULT_PORTABLES};
use system_bridge::{settings::NewCameraEvent, NativeUi, SystemBridgePlugin};
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
        app_config.migrate_inputs();

        Self {
            app_config,
            arguments,
            #[cfg(not(target_arch = "wasm32"))]
            crash_file,
            #[cfg(target_arch = "wasm32")]
            wasm_loader_handle,
        }
    }

    /// The realm the engine boots into: an explicit --realm, else the home realm.
    /// --realm is a startup param (like the web's ?realm=) and is deliberately NOT
    /// merged into the AppConfig — see the home_realm field docs.
    pub fn boot_server(&self) -> String {
        self.arguments
            .launch
            .realm
            .clone()
            .unwrap_or_else(|| self.app_config.home_realm())
    }

    /// The parcel the player spawns at: an explicit --position, else the home parcel.
    /// Same contract as [`Self::boot_server`].
    pub fn boot_location(&self) -> IVec2 {
        self.arguments
            .location()
            .unwrap_or_else(|| self.app_config.home_location())
    }
}

/// The spawn parcel resolved by [`DecentralandAppConfig::boot_location`] — a distinct
/// resource so the AppConfig resource (rewritten wholesale to disk on settings changes)
/// never carries a one-off --position as home.
#[derive(Resource)]
pub struct BootLocation(pub IVec2);

/// The native command line. The launch parameters shared with the web build are
/// [`LaunchOptions`] (declared once, in system_api_types — its doc comments are the `--help`
/// text and the web param table); everything here is native-only. Values that are derived
/// rather than given (test mode, the ui scene, the spawn parcel) are methods.
#[derive(clap::Parser, Default)]
#[command(name = "decentra-bevy", about = "Decentraland Bevy Explorer")]
pub struct DecentralandArguments {
    #[command(flatten)]
    pub launch: LaunchOptions,
    /// Echo scene logs to the console
    #[arg(long = "scene_log_to_console", display_order = 6)]
    pub scene_log_to_console: bool,
    /// Pause that scene's js runtime until a debugger (e.g. chrome://inspect) attaches. Needs
    /// `--features inspect`
    #[arg(long, value_name = "scene_hash", display_order = 10)]
    pub inspect: Option<String>,
    /// Target fps (default 60; overridden by the refresh rate when vsync is on). Also `/fps`.
    /// Current run only - use settings for persistence.
    #[arg(long = "fps", value_name = "n", display_order = 14)]
    pub fps_target: Option<usize>,
    /// Run the portable/startup scenes in preview mode
    #[arg(long = "ui-preview", display_order = 15)]
    pub startup_scenes_preview: bool,
    /// Max simultaneous scene-javascript threads (default 4). Also `/scene_threads`
    #[arg(long = "threads", value_name = "n", display_order = 16)]
    pub scene_threads: Option<usize>,
    /// Automated scene test mode: headless, no HUD (implied by --test_scenes)
    #[arg(long = "testing", display_order = 18)]
    pub testing: bool,
    /// Run the scene test harness over those parcels and exit; a parcel may carry
    /// `/allowed/failures`
    #[arg(long = "test_scenes", value_name = "x,y;x,y", display_order = 19)]
    pub test_scenes: Option<TestScenes>,
    /// Vsync (default off). Current run only - use settings for persistence.
    #[arg(long, value_name = "true|false", display_order = 20)]
    pub vsync: Option<bool>,
    /// Scene load distance in meters (default 100). Also `/scene_distance`. Current run only -
    /// use settings for persistence.
    #[arg(long = "distance", value_name = "m", display_order = 21)]
    pub scene_load_distance: Option<f32>,
    /// Extra distance before scenes are unloaded. Current run only - use settings for
    /// persistence.
    #[arg(long = "unload", value_name = "m", display_order = 22)]
    pub scene_unload_extra_distance: Option<f32>,
    /// Imposter distances. Current run only - use settings for persistence.
    #[arg(
        long = "impost",
        value_name = "d1,d2,…",
        value_delimiter = ',',
        display_order = 23
    )]
    pub scene_imposter_distances: Option<Vec<f32>>,
    /// Imposter multisampling
    #[arg(long = "impost_multi", value_name = "true|false", display_order = 24)]
    pub scene_imposter_multisample: Option<bool>,
    /// Imposter local baking speed: f(ull), h(alf), q(uarter) or o(ff)
    #[arg(long = "bake", value_name = "f|h|q|o", value_parser = parse_bake, display_order = 25)]
    pub scene_imposter_bake: Option<SceneImposterBake>,
    /// Show the system info overlay
    #[arg(long = "sysinfo", display_order = 26)]
    pub sysinfo_visible: bool,
    /// Disable avatar rendering
    #[arg(long = "no_avatar", display_order = 27)]
    pub no_avatar: bool,
    /// Disable gltf loading
    #[arg(long = "no_gltf", display_order = 28)]
    pub no_gltf: bool,
    /// Disable distance fog
    #[arg(long = "no_fog", display_order = 29)]
    pub no_fog: bool,
    /// Force the engine-drawn login back on
    #[arg(long = "builtin-login", display_order = 30)]
    pub login: bool,
    /// Force the engine-drawn emote wheel back on
    #[arg(long = "builtin-emotes", display_order = 31)]
    pub emote_wheel: bool,
    /// Force the engine-drawn chat back on
    #[arg(long = "builtin-chat", display_order = 32)]
    pub chat: bool,
    /// Force the engine-drawn permission prompts back on
    #[arg(long = "builtin-perms", display_order = 33)]
    pub permissions: bool,
    /// Force the engine-drawn nametags back on
    #[arg(long = "builtin-nametags", display_order = 34)]
    pub nametags: bool,
    /// Force the engine-drawn tooltips back on
    #[arg(long = "builtin-tooltips", display_order = 35)]
    pub tooltips: bool,
    /// Force the engine-drawn loading scene ui back on
    #[arg(long = "builtin-loading-scene-ui", display_order = 36)]
    pub loading_scene: bool,
    /// run the react HUD (native: the CEF overlay). False when an explicit --system-scene opted
    /// out in favour of the engine-side ui, and on wasm (the react page hosts the engine itself).
    #[arg(skip)]
    pub hud: bool,
}

fn parse_bake(bake: &str) -> Result<SceneImposterBake, String> {
    match bake.to_lowercase().chars().next() {
        None | Some('f') => Ok(SceneImposterBake::FullSpeed),
        Some('h') => Ok(SceneImposterBake::HalfSpeed),
        Some('q') => Ok(SceneImposterBake::QuarterSpeed),
        Some('o') => Ok(SceneImposterBake::Off),
        _ => Err(format!(
            "'{bake}' is not a valid bake argument. Valid values are 'f', 'h', 'q', or 'o'."
        )),
    }
}

impl DecentralandArguments {
    pub fn test_mode(&self) -> bool {
        self.testing || self.test_scenes.is_some()
    }

    /// The super-user ui scene: `--system-scene` / `?systemScene=`, less the `none` opt-out.
    pub fn ui_scene(&self) -> Option<&str> {
        self.launch
            .system_scene
            .as_deref()
            .filter(|scene| *scene != "none")
    }

    /// `--position` / `?position=` as a parcel; main.rs rejects an unparseable one up front.
    pub fn location(&self) -> Option<IVec2> {
        self.launch
            .position
            .as_deref()
            .and_then(|position| IVec2Arg::from_str(position).ok())
            .map(|parcel| parcel.0)
    }

    /// `--portables` / `?portables=`, else the default set.
    pub fn startup_scenes(&self) -> Vec<StartupScene> {
        self.launch
            .portables
            .as_deref()
            .unwrap_or(DEFAULT_PORTABLES)
            .split(';')
            .map(|source| StartupScene {
                source: source.to_owned(),
                super_user: false,
                preview: self.startup_scenes_preview,
                hot_reload: None,
                hash: None,
            })
            .collect()
    }
}

/// Whether a realm URL points at the local machine (localhost / loopback IP).
fn is_loopback_realm(realm: &str) -> bool {
    let after_scheme = realm.split("://").last().unwrap_or(realm);
    let host = after_scheme
        .split(['/', ':'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    host == "localhost" || host == "127.0.0.1" || host == "::1" || host == "[::1]"
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

        #[cfg(all(not(target_arch = "wasm32"), feature = "ffmpeg"))]
        media::init_ffmpeg();

        // we use kira for audio source asset management, regardless of native / wasm
        app.add_plugins(bevy_kira_audio::AudioPlugin);

        // POC: react-web HUD composited in-engine from CEF offscreen rendering. Skipped in test
        // mode (automated scene tests run headless and must not boot CEF or gate input) and when
        // an explicit --system-scene opted out of the HUD in favour of the engine-side ui.
        #[cfg(all(not(target_arch = "wasm32"), feature = "react-hud-cef"))]
        if decentraland_app_config.arguments.hud && !decentraland_app_config.arguments.test_mode() {
            app.add_plugins(react_hud_cef::ReactHudCefPlugin {
                // a non-default boot server (explicit --realm or a configured home realm)
                // IS the destination: injected into the page URL as ?realm= so the HUD skips
                // its places picker (parity with ?realm= on web). On the stock default the
                // param is omitted so the picker shows — and the HUD's own default-realm
                // assumption then matches the realm the engine actually booted.
                server: (decentraland_app_config.boot_server()
                    != AppConfig::default().home_realm())
                .then(|| decentraland_app_config.boot_server()),
            });
        }

        let version_hash = version();
        let version = format!("{VERSION} ({version_hash})");

        info!("Bevy-Explorer version {}", version);

        let boot_server = map_realm_name(&decentraland_app_config.boot_server());
        let boot_location = BootLocation(decentraland_app_config.boot_location());
        // Show out-of-bounds geometry in preview, on a loopback realm (local dev) and in
        // the editor, never on a public realm. Computed before boot_server moves.
        let editor_mode = decentraland_app_config.arguments.launch.editor;
        let show_out_of_bounds = editor_mode
            || decentraland_app_config.arguments.launch.preview
            || is_loopback_realm(&boot_server);

        // Resources
        app.insert_resource(Version(version))
            .insert_resource(TestingData {
                inspect_hash: decentraland_app_config.arguments.inspect.clone(),
                test_mode: decentraland_app_config.arguments.test_mode(),
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
                    .launch
                    .preview
                    .then_some(boot_server),
                is_preview: decentraland_app_config.arguments.launch.preview,
                preview_parcel: None,
            })
            .insert_resource(EditorMode(editor_mode))
            .insert_resource(ShowOutOfBounds(show_out_of_bounds))
            .insert_resource(SceneLoadDistance {
                load: if decentraland_app_config.arguments.launch.preview {
                    1.0
                } else {
                    decentraland_app_config.app_config.scene_load_distance
                },
                unload: if decentraland_app_config.arguments.launch.preview {
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
                    * if decentraland_app_config.arguments.launch.preview {
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

        let mut startup_scenes = decentraland_app_config.arguments.startup_scenes();

        if let Some(source) = decentraland_app_config.arguments.ui_scene() {
            app.insert_resource(NativeUi {
                login: decentraland_app_config.arguments.login,
                emote_wheel: decentraland_app_config.arguments.emote_wheel,
                chat: decentraland_app_config.arguments.chat,
                permissions: decentraland_app_config.arguments.permissions,
                nametags: decentraland_app_config.arguments.nametags,
                tooltips: decentraland_app_config.arguments.tooltips,
                loading_scene: decentraland_app_config.arguments.loading_scene,
            });
            startup_scenes.insert(
                0,
                StartupScene {
                    source: source.to_owned(),
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
                nametags: true,
                tooltips: true,
                loading_scene: true,
            });
        }

        // POC: the react-web overlay is the HUD — turn off the engine's native UI so it doesn't
        // render its own login/chat/etc. behind the webview. (Overrides the inserts above.)
        // Test mode and an explicit --system-scene keep the native UI: the HUD plugin is skipped there.
        #[cfg(all(not(target_arch = "wasm32"), feature = "react-hud-cef"))]
        if decentraland_app_config.arguments.hud && !decentraland_app_config.arguments.test_mode() {
            app.insert_resource(NativeUi {
                login: false,
                emote_wheel: false,
                chat: false,
                permissions: false,
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

        if let Some(endpoint) = decentraland_app_config
            .arguments
            .launch
            .pulse_server
            .clone()
        {
            app.insert_resource(comms::pulse::plugin::PulseEndpointOverride(endpoint));
        }
        if let Some(source) = &decentraland_app_config.arguments.launch.imposter_source {
            imposters::imposter_spec::set_source(source);
        }

        // Create copies of structs that still need to be accessed
        // and add AppConfig as a resource
        let graphics_config = decentraland_app_config.app_config.graphics.clone();
        app.insert_resource(decentraland_app_config.app_config.audio.clone());
        app.insert_resource(boot_location);
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
            .add_plugins(EmbedAssetsPlugin)
            .add_plugins(ParticleSystemPlugin)
            .add_plugins(LivestreamManagerPlugin)
            .add_plugins(media::plugin::MediaPlugin);

        if !decentraland_app_config.arguments.launch.preview {
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
        if (graphics_config.log_fps || decentraland_app_config.arguments.launch.preview)
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
    boot_location: Res<BootLocation>,
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
                8.0 + 16.0 * boot_location.0.x as f32,
                8.0,
                -8.0 + -16.0 * boot_location.0.y as f32,
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
    base_app_config
        .graphics
        .vsync
        .replace_if_some(arguments.vsync);
    base_app_config
        .graphics
        .log_fps
        .replace_if_some(arguments.launch.log_fps);
    base_app_config
        .graphics
        .fps_target
        .replace_if_some(arguments.fps_target);
    base_app_config
        .graphics
        .gpu_bytes_per_frame
        .replace_if_some(arguments.launch.gpu_bytes_per_frame);

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
            preview: decentraland_app_config.arguments.launch.preview,
            starting_realm: Some(map_realm_name(&decentraland_app_config.boot_server())),
            content_server_override: decentraland_app_config
                .arguments
                .launch
                .content_server
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
            preview: decentraland_app_config.arguments.launch.preview,
            starting_realm: Some(map_realm_name(&decentraland_app_config.boot_server())),
            content_server_override: decentraland_app_config
                .arguments
                .launch
                .content_server
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
