//! What each launch option DOES — once, for every binary. `LaunchOptions` / `ClientOptions`
//! (crates/system_api_types) declare the options; this is where they take effect, so a binary
//! that flattens a struct (native, headless) or deserialises it (web) gets each option's
//! behaviour by calling the two phases below rather than re-implementing them:
//!
//! 1. [`latch`] before anything else — process-wide globals that url composition reads
//! 2. [`apply`] (every binary) and [`apply_client`] (the rendering clients: native and web) at
//!    app assembly — resources and plugins
//!
//! An option never writes into the app config, which is persisted: where one overrides a
//! config setting, `apply` reads the option first and the config second. Each `apply`
//! destructures its struct without `..`, so a new field fails to compile until it is given a
//! meaning here (or, for the destination and scene set, explicitly left to the binary).

use bevy::{
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    prelude::*,
    render::render_asset::RenderAssetBytesPerFrame,
};
use common::structs::{AppConfig, EditorMode, PreviewMode};
use system_api_types::launch_options::{ClientOptions, LaunchOptions};

/// The base domain every backend host composes from — so this runs before
/// `AppConfig::default()`, which composes the default realm.
pub fn latch(launch: &LaunchOptions) -> Result<(), String> {
    if let Some(domain) = &launch.base_domain {
        common::base_domain::set(domain)?;
    }
    Ok(())
}

/// `boot_server` is the realm the binary decided to boot into (mapped through
/// `map_realm_name`), `config` the app config the options override.
pub fn apply(app: &mut App, launch: &LaunchOptions, config: &AppConfig, boot_server: &str) {
    let LaunchOptions {
        // the destination is the binary's: boot server / location, the IpfsIoPlugin's realm
        // and content server
        realm: _,
        position: _,
        content_server: _,
        // latch
        base_domain: _,
        preview,
        pulse_server,
        log_fps,
    } = launch;

    app.insert_resource(PreviewMode {
        server: preview.then(|| boot_server.to_owned()),
        is_preview: *preview,
        preview_parcel: None,
    });

    if let Some(endpoint) = pulse_server {
        app.insert_resource(comms::pulse::plugin::PulseEndpointOverride(
            endpoint.clone(),
        ));
    }

    if log_fps.unwrap_or(config.graphics.log_fps) {
        // the HUD adds the frame timing too
        if !app.is_plugin_added::<FrameTimeDiagnosticsPlugin>() {
            app.add_plugins(FrameTimeDiagnosticsPlugin::default());
        }
        app.add_plugins(LogDiagnosticsPlugin::default());
    }
}

/// `launch` for the shared options' client-only effects.
pub fn apply_client(
    app: &mut App,
    launch: &LaunchOptions,
    client: &ClientOptions,
    config: &AppConfig,
) {
    let ClientOptions {
        // the scene set is the binary's: the ui scene and the startup scenes
        system_scene: _,
        portables: _,
        editor,
        imposter_source,
        gpu_bytes_per_frame,
    } = client;

    app.insert_resource(EditorMode(*editor));

    // the preview stats and sysinfo panels (system_ui) read the frame rate
    if launch.preview && !app.is_plugin_added::<FrameTimeDiagnosticsPlugin>() {
        app.add_plugins(FrameTimeDiagnosticsPlugin::default());
    }

    if let Some(source) = imposter_source {
        imposters::imposter_spec::set_source(source);
    }

    let gpu_bytes_per_frame = gpu_bytes_per_frame.unwrap_or(config.graphics.gpu_bytes_per_frame);
    if gpu_bytes_per_frame > 0 {
        app.insert_resource(RenderAssetBytesPerFrame::new_with_priorities(
            gpu_bytes_per_frame,
        ));
    }
}
