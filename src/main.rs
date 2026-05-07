#![cfg_attr(not(feature = "console"), windows_subsystem = "windows")]

use std::{error::Error, fmt::Display, fs::File, io::Write, path::PathBuf, sync::OnceLock};

use bevy::{log::LogPlugin, prelude::*};
#[cfg(not(debug_assertions))]
use build_time::build_time_utc;
use common::structs::{AppConfig, IVec2Arg, SceneImposterBake, StartupScene};
use dcl_deno_ipc::init_runtime;
use mimalloc::MiMalloc;
use webgpu_build::{DecentralandApp, DecentralandAppConfig, DecentralandArguments};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;
static SESSION_LOG: OnceLock<String> = OnceLock::new();

fn main() {
    decentraland_log_file();
    create_logs_folder();
    create_log_files();

    let decentraland_app = DecentralandApp::new(LogPlugin {
        filter: "wgpu=error,naga=error,bevy_animation=error,matrix=error".to_string(),
        custom_layer: move |_| {
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
    });

    // requires local version of `bevy_mod_debugdump` due to once_cell version conflict.
    // probably resolved by updating deno. TODO: add feature flag for this after bumping deno
    // bevy_mod_debugdump::print_main_schedule(&mut app);
    #[cfg(not(feature = "console"))]
    log_panics::init();

    // initialize v8 runtime from main thread
    init_runtime().unwrap();

    match decentraland_app_config() {
        Ok(decentraland_app_config) => {
            decentraland_app.build(decentraland_app_config).run();
        }
        Err(UserError(false)) => panic!("Fatal error while building application configurations."),
        Err(UserError(true)) => {
            // Non need to generate a crash report if the failure
            // is due to an user error
        }
    };

    // Graceful exits don't need to have the log sent to the analytics server
    // so we remove the touch file
    let _ = std::fs::remove_file(format!("{}.touch", SESSION_LOG.get().unwrap()));
}

fn decentraland_app_config() -> Result<DecentralandAppConfig, UserError> {
    let app_config = decentraland_serialized_app_config();
    let arguments = decentraland_app_arguments()?;
    let crash_file = decentraland_crash_file();

    Ok(DecentralandAppConfig::new(
        app_config, arguments, crash_file,
    ))
}

fn decentraland_serialized_app_config() -> AppConfig {
    let config_file = platform::project_directories()
        .unwrap()
        .config_dir()
        .join("config.json");
    let base_config: AppConfig = std::fs::read(&config_file)
        .ok()
        .and_then(|f| {
            info!("config file loaded from {config_file:?}");
            serde_json::from_slice(&f)
                .map_err(|e| warn!("failed to parse config.json: {e}"))
                .ok()
        })
        .unwrap_or_else(|| {
            warn!("config file not found at {config_file:?}, generating default");
            Default::default()
        });

    base_config
}

fn decentraland_app_arguments() -> Result<DecentralandArguments, UserError> {
    let mut args = pico_args::Arguments::from_env();

    let test_scenes = args.value_from_str("--test_scenes").ok();
    let startup_scenes_preview = args.contains("--ui-preview");

    let dcl_args = DecentralandArguments {
        server: args.value_from_str("--server").ok(),
        content_server_override: args.value_from_str("--content-server").ok(),
        location: args
            .value_from_str::<_, IVec2Arg>("--location")
            .ok()
            .map(|location_arg| location_arg.0),
        startup_scenes: args
            .value_from_str::<_, String>("--portables")
            .map(|p| {
                p.split(";")
                    .map(|scene| StartupScene {
                        source: scene.to_owned(),
                        super_user: false,
                        preview: startup_scenes_preview,
                        hot_reload: None,
                        hash: None,
                    })
                    .collect::<Vec<_>>()
            })
            .ok(),
        ui_scene: args
            .value_from_str("--ui")
            .ok()
            .or_else(|| {
                Some(String::from(
                    "https://dcl-regenesislabs.github.io/bevy-ui-scene/BevyUiScene",
                ))
            })
            .filter(|scene| scene != "none"),
        scene_params: args.value_from_str("--params").ok(),
        scene_threads: args.value_from_str("--threads").ok(),
        scene_load_distance: args.value_from_str("--distance").ok(),
        scene_unload_extra_distance: args.value_from_str("--unload").ok(),
        scene_imposter_bake: args.value_from_str("--bake").ok().map(|bake: String| {
            match bake.to_lowercase().chars().next() {
                None | Some('f') => SceneImposterBake::FullSpeed,
                Some('h') => SceneImposterBake::HalfSpeed,
                Some('q') => SceneImposterBake::QuarterSpeed,
                Some('o') => SceneImposterBake::Off,
                _ => panic!(
                    "'{}' is not a valid bake argument. Valid values are 'f', 'h', 'q', or 'o'.",
                    bake
                ),
            }
        }),
        scene_imposter_distances: args
            .value_from_str("--impost")
            .ok()
            .map(|distances: String| {
                distances
                    .split(",")
                    .map(str::parse::<f32>)
                    .collect::<Result<Vec<f32>, _>>()
                    .unwrap()
            }),
        scene_imposter_multisample: args.value_from_str("--impost_multi").ok(),
        vsync: args.value_from_str("--vsync").ok(),
        fps_target: args.value_from_str::<_, usize>("--fps").ok(),
        gpu_bytes_per_frame: args
            .value_from_str::<_, usize>("--gpu_bytes_per_frame")
            .ok(),
        is_preview: args.contains("--preview"),
        sysinfo_visible: args.contains("--sysinfo"),
        scene_log_to_console: args.contains("--scene_log_to_console"),
        startup_scenes_preview,
        no_avatar: args.contains("--no_avatar"),
        no_gltf: args.contains("--no_gltf"),
        no_fog: args.contains("--no_fog"),
        log_fps: args.value_from_str("--log_fps").ok(),
        inspect: args.value_from_str("--inspect").ok(),
        test_mode: args.contains("--testing") || test_scenes.is_some(),
        test_scenes,
        login: args.contains("--builtin-login"),
        emote_wheel: args.contains("--builtin-emotes"),
        chat: args.contains("--builtin-chat"),
        permissions: args.contains("--builtin-perms"),
        profile: args.contains("--builtin-profile"),
        nametags: args.contains("--builtin-nametags"),
        tooltips: args.contains("--builtin-tooltips"),
        loading_scene: args.contains("--builtin-loading-scene-ui"),
    };

    let remaining = args.finish();
    if !remaining.is_empty() {
        error!(
            "failed to parse args: {}",
            remaining
                .iter()
                .map(|arg| arg.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ")
        );
        return Err(UserError(true));
    }

    Ok(dcl_args)
}

#[inline(always)]
fn data_local_dir() -> PathBuf {
    let dirs = platform::project_directories().unwrap();
    dirs.data_local_dir().to_owned()
}

fn create_logs_folder() {
    let log_dir = data_local_dir();
    std::fs::create_dir_all(log_dir).unwrap();
}

fn create_log_files() {
    File::create(SESSION_LOG.get().unwrap())
        .expect("failed to create log file")
        .write_all(format!("{}\n\n", SESSION_LOG.get().unwrap()).as_bytes())
        .expect("failed to create log file");

    File::create(format!("{}.touch", SESSION_LOG.get().unwrap())).unwrap();
    println!("log file: {}", SESSION_LOG.get().unwrap());
}

/// Generate the file name for the log files of current instance
/// and saves it to [`SESSION_LOG`]
fn decentraland_log_file() {
    let log_dir = data_local_dir();
    let session_time: chrono::DateTime<chrono::Utc> = chrono::DateTime::from_timestamp_millis(
        web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64,
    )
    .unwrap();
    let session_log = log_dir.join(format!("{}.log", session_time.format("%Y%m%d-%H%M%S")));
    SESSION_LOG
        .set(session_log.to_string_lossy().into_owned())
        .unwrap();
}

fn decentraland_crash_file() -> Option<PathBuf> {
    let log_dir = data_local_dir();
    std::fs::read_dir(log_dir)
        .unwrap()
        .filter_map(|f| f.ok())
        .find(|f| f.path().extension().map(|oss| oss.to_string_lossy()) == Some("touch".into()))
        .map(|f| {
            f.path()
                .parent()
                .unwrap()
                .join(f.path().file_stem().unwrap())
        })
}

#[derive(Debug)]
struct UserError(bool);

impl Display for UserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self(true) => write!(f, "Method failed due to user error."),
            Self(false) => write!(f, "Method failed due to application error."),
        }
    }
}

impl Error for UserError {}
