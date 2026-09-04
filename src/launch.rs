//! What each launch option DOES — once, for every binary. `LaunchOptions` / `ClientOptions`
//! (crates/system_api_types) declare the options; this is where they take effect, so a binary
//! that flattens a struct (native, headless) or deserialises it (web) gets each option's
//! behaviour by calling the three phases below rather than re-implementing them:
//!
//! 1. `latch` before anything else — process-wide globals that url composition reads
//! 2. `configure` on the app config — option overrides of persisted settings
//! 3. `apply` at app assembly — resources and plugins
//!
//! [`shared`] is `LaunchOptions` (every binary), [`client`] is `ClientOptions` (the rendering
//! clients: native and web call the top-level functions, which do both; headless calls
//! [`shared`] alone). Each `apply` destructures its struct without `..`, so a new field fails
//! to compile until it is given a meaning in one of the phases (or, for the destination and
//! scene set, explicitly left to the binary).

use bevy::prelude::*;
use common::structs::AppConfig;
use system_api_types::launch_options::{ClientOptions, LaunchOptions};

pub fn latch(launch: &LaunchOptions, client: &ClientOptions) -> Result<(), String> {
    shared::latch(launch)?;
    client::latch(client);
    Ok(())
}

pub fn configure(config: &mut AppConfig, launch: &LaunchOptions, client: &ClientOptions) {
    shared::configure(config, launch);
    client::configure(config, client);
}

pub fn apply(
    app: &mut App,
    launch: &LaunchOptions,
    client: &ClientOptions,
    config: &AppConfig,
    boot_server: &str,
) {
    shared::apply(app, launch, config, boot_server);
    client::apply(app, client);
}

pub mod shared {
    use bevy::{
        diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
        prelude::*,
    };
    use common::structs::{AppConfig, PreviewMode};
    use system_api_types::launch_options::LaunchOptions;

    /// The base domain every backend host composes from — so this runs before
    /// `AppConfig::default()`, which composes the default realm.
    pub fn latch(launch: &LaunchOptions) -> Result<(), String> {
        if let Some(domain) = &launch.base_domain {
            common::base_domain::set(domain)?;
        }
        Ok(())
    }

    pub fn configure(config: &mut AppConfig, launch: &LaunchOptions) {
        if let Some(log_fps) = launch.log_fps {
            config.graphics.log_fps = log_fps;
        }
    }

    /// `boot_server` is the realm the binary decided to boot into (mapped through
    /// `map_realm_name`), `config` the app config after [`configure`].
    pub fn apply(app: &mut App, launch: &LaunchOptions, config: &AppConfig, boot_server: &str) {
        let LaunchOptions {
            // the destination is the binary's: boot server / location, the IpfsIoPlugin's realm
            // and content server
            realm: _,
            position: _,
            content_server: _,
            // latch
            base_domain: _,
            // configure
            log_fps: _,
            preview,
            pulse_server,
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

        // frame timing feeds the fps log and the preview-mode sysinfo panel; the HUD adds it too
        if (config.graphics.log_fps || *preview)
            && !app.is_plugin_added::<FrameTimeDiagnosticsPlugin>()
        {
            app.add_plugins(FrameTimeDiagnosticsPlugin::default());
        }
        if config.graphics.log_fps {
            app.add_plugins(LogDiagnosticsPlugin::default());
        }
    }
}

pub mod client {
    use bevy::prelude::*;
    use common::structs::{AppConfig, EditorMode};
    use system_api_types::launch_options::ClientOptions;

    pub fn latch(client: &ClientOptions) {
        if let Some(source) = &client.imposter_source {
            imposters::imposter_spec::set_source(source);
        }
    }

    pub fn configure(config: &mut AppConfig, client: &ClientOptions) {
        if let Some(bytes) = client.gpu_bytes_per_frame {
            config.graphics.gpu_bytes_per_frame = bytes;
        }
    }

    pub fn apply(app: &mut App, client: &ClientOptions) {
        let ClientOptions {
            // the scene set is the binary's: the ui scene and the startup scenes
            system_scene: _,
            portables: _,
            // latch
            imposter_source: _,
            // configure
            gpu_bytes_per_frame: _,
            editor,
        } = client;

        app.insert_resource(EditorMode(*editor));
    }
}
