//! What each shared launch option DOES — once, for every binary. `LaunchOptions`
//! (crates/system_api_types) declares the options; this is where they take effect, so a
//! binary that flattens the struct (native, headless) or deserialises it (web) gets each
//! option's behaviour by calling the three phases below rather than re-implementing them:
//!
//! 1. [`latch`] before anything else — process-wide globals that url composition reads
//! 2. [`configure`] on the app config — option overrides of persisted settings
//! 3. [`apply`] at app assembly — resources and plugins
//!
//! [`apply`] destructures the struct without `..`, so a new field fails to compile until it is
//! given a meaning in one of the phases (or, for the destination and scene set, explicitly left
//! to the binary).

use bevy::{
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    prelude::*,
};
use common::structs::{AppConfig, EditorMode, PreviewMode};
use system_api_types::launch_options::LaunchOptions;

/// Process-wide globals: the base domain every backend host composes from (so this runs before
/// `AppConfig::default()`, which composes the default realm) and the imposter store.
pub fn latch(launch: &LaunchOptions) -> Result<(), String> {
    if let Some(domain) = &launch.base_domain {
        common::base_domain::set(domain)?;
    }
    if let Some(source) = &launch.imposter_source {
        imposters::imposter_spec::set_source(source);
    }
    Ok(())
}

/// Option overrides of persisted settings.
pub fn configure(config: &mut AppConfig, launch: &LaunchOptions) {
    if let Some(log_fps) = launch.log_fps {
        config.graphics.log_fps = log_fps;
    }
    if let Some(bytes) = launch.gpu_bytes_per_frame {
        config.graphics.gpu_bytes_per_frame = bytes;
    }
}

/// Resources and plugins. `boot_server` is the realm the binary decided to boot into (mapped
/// through `map_realm_name`), `config` the app config after [`configure`].
pub fn apply(app: &mut App, launch: &LaunchOptions, config: &AppConfig, boot_server: &str) {
    let LaunchOptions {
        // the destination and the scene set are the binary's: boot server / location, the
        // startup scenes, the IpfsIoPlugin's realm and content server
        realm: _,
        position: _,
        system_scene: _,
        portables: _,
        content_server: _,
        // latch
        base_domain: _,
        imposter_source: _,
        // configure
        log_fps: _,
        gpu_bytes_per_frame: _,
        preview,
        editor,
        pulse_server,
    } = launch;

    app.insert_resource(PreviewMode {
        server: preview.then(|| boot_server.to_owned()),
        is_preview: *preview,
        preview_parcel: None,
    })
    .insert_resource(EditorMode(*editor));

    if let Some(endpoint) = pulse_server {
        app.insert_resource(comms::pulse::plugin::PulseEndpointOverride(
            endpoint.clone(),
        ));
    }

    // frame timing feeds the fps log and the preview-mode sysinfo panel; the HUD adds it too
    if (config.graphics.log_fps || *preview) && !app.is_plugin_added::<FrameTimeDiagnosticsPlugin>()
    {
        app.add_plugins(FrameTimeDiagnosticsPlugin::default());
    }
    if config.graphics.log_fps {
        app.add_plugins(LogDiagnosticsPlugin::default());
    }
}
