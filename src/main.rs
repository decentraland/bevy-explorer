#![cfg_attr(not(feature = "console"), windows_subsystem = "windows")]

use std::{
    error::Error, fmt::Display, fs::File, io::Write, path::PathBuf, str::FromStr, sync::OnceLock,
};

use bevy::{log::LogPlugin, prelude::*};
use clap::Parser;
use common::structs::{AppConfig, IVec2Arg};
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
        filter: "wgpu=error,naga=error,bevy_animation=error,matrix=error,symphonia=warn"
            .to_string(),
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

    // args first, so --help and a bad flag answer without spawning the scene runtime
    match decentraland_app_config() {
        Ok(decentraland_app_config) => {
            // initialize v8 runtime from main thread
            init_runtime().unwrap();
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
    let arguments = decentraland_app_arguments()?;
    let app_config = decentraland_serialized_app_config();
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
    let mut base_config: AppConfig = std::fs::read(&config_file)
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
    base_config.reset_outdated_settings();

    base_config
}

fn decentraland_app_arguments() -> Result<DecentralandArguments, UserError> {
    let mut args = match DecentralandArguments::try_parse() {
        Ok(args) => args,
        // --help, or a bad flag: clap's own message (an error also lands in the session log via
        // tracing), and no crash report
        Err(e) => {
            if e.use_stderr() {
                error!("{e}");
            } else {
                let _ = e.print();
            }
            return Err(UserError(true));
        }
    };

    if let Some(domain) = &args.launch.base_domain {
        common::base_domain::set(domain).map_err(|e| {
            error!("{e}");
            UserError(true)
        })?;
    }
    if let Some(position) = &args.launch.position {
        IVec2Arg::from_str(position).map_err(|e| {
            error!("--location {position}: {e}");
            UserError(true)
        })?;
    }

    // An explicit --ui (a scene source, or "none" for the engine's builtin ui) opts out of the
    // react HUD entirely — the given ui scene drives instead (see lib.rs).
    args.hud = args.launch.system_scene.is_none();
    if args.launch.system_scene.is_none() {
        args.launch.system_scene = default_ui_scene();
    }
    Ok(args)
}

/// react HUD builds default to the bundled bridge-scene static export (a file realm, loaded with
/// no server; `npm run bundle:native` in react-web generates it). Checked against the cwd
/// (packaged runs) and the compile-time checkout dir (dev `cargo run` from any directory). If
/// absent there is NO ui scene — the react HUD's built-in fallback relay covers login/chat/loading.
fn default_ui_scene() -> Option<String> {
    #[cfg(feature = "react-hud-cef")]
    {
        for root in native_hud_roots() {
            let local = root.join("assets/bridge-scene/BevyExplorerUI");
            if local.join("about").is_file() {
                return Some(local.to_string_lossy().into_owned());
            }
        }
        None
    }
    #[cfg(not(feature = "react-hud-cef"))]
    Some(String::from(
        "https://dcl-regenesislabs.github.io/bevy-ui-scene/BevyUiScene",
    ))
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

// Roots to search for the bundled native HUD files: cwd, the executable's directory (packaged
// layouts), and the dev checkout (`cargo run` from any directory).
#[cfg(feature = "react-hud-cef")]
fn native_hud_roots() -> Vec<std::path::PathBuf> {
    let mut roots = vec![
        std::path::PathBuf::from("."),
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")),
    ];
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf))
    {
        roots.push(dir);
    }
    roots
}
