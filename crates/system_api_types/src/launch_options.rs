//! The launch parameters, defined ONCE, here. Natively each field is a `--flag` (clap; the doc
//! comment is its `--help` text), on web the same field is a key of the `engine_run` options
//! object (serde, camelCase) and an entry-url query param. The web param table (`web_params`)
//! is derived from these structs' clap metadata plus a per-field delivery annotation, and the
//! host page is generated from that table — so a parameter is declared here and nowhere else.
//!
//! Two structs, by which binaries take them:
//! - [`LaunchOptions`]: every binary — native, web and headless flatten it in
//! - [`ClientOptions`]: the rendering clients only (native and web); headless never sees them,
//!   so they are unknown flags there
//!
//! What each one DOES is `src/launch.rs` in the root crate (`apply` / `apply_client`). Field
//! names are the web names, and the native flags are the same names in kebab-case. Everything is
//! optional: absent = the engine's default. Native-only flags live on `DecentralandArguments`
//! (src/lib.rs).

use serde::{Deserialize, Serialize};

use crate::services::ServiceOverrides;

#[derive(clap::Args, Deserialize, Serialize, Default, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct LaunchOptions {
    /// Realm to boot into; absent = the persisted home realm or default realm
    #[arg(long, value_name = "url", display_order = 1)]
    pub realm: Option<String>,

    /// Spawn parcel as `x,y`; absent = the home parcel, or the realm's spawn point
    #[arg(
        long,
        value_name = "x,y",
        allow_hyphen_values = true,
        display_order = 2
    )]
    pub position: Option<String>,

    /// Scene preview mode: hot-reloading, no failed-asset backoff, plain-http fetches allowed,
    /// realm fixed.
    #[arg(long, display_order = 3)]
    pub preview: bool,

    /// The base domain for all services (comms, profiles, etc); absent = the hosting origin (on
    /// web), or decentraland.org
    #[arg(long, value_name = "domain", display_order = 4)]
    pub base_domain: Option<String>,

    /// Override the content server only
    #[arg(long, value_name = "url", display_order = 7)]
    pub content_server: Option<String>,

    /// Log the frame rate to the console
    #[arg(long, value_name = "true|false", display_order = 13)]
    pub log_fps: Option<bool>,

    /// Per-service url overrides (`services.rs`): flags natively, keys of the `engine_run`
    /// object on web (the page resolves the same overrides for its own fetches first).
    #[serde(flatten)]
    #[command(flatten)]
    pub services: ServiceOverrides,
}

/// The options only a rendering client (native, web) has a use for.
#[derive(clap::Args, Deserialize, Serialize, Default, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct ClientOptions {
    /// Embedded in a scene editor (creator hub). Set by editor front-ends.
    #[arg(long, display_order = 5)]
    pub editor: bool,

    /// Base url of the imposter store; absent = the default store. The realm-keyed path under
    /// it is the same as the default store's
    #[arg(long, value_name = "url", display_order = 9)]
    pub imposter_source: Option<String>,

    /// Super-user ui scene source, or `none` for no ui scene. The engine trusts it completely.
    /// Absent = the default bridge scene for the react HUD; any explicit value opts out of the
    /// HUD
    #[arg(long, value_name = "scene|none", display_order = 11)]
    pub system_scene: Option<String>,

    /// `;`-separated portable/startup scene sources; absent = `basiccontroller.dcl.eth`
    /// (DEFAULT_PORTABLES)
    #[arg(long, value_name = "a;b", display_order = 12)]
    pub portables: Option<String>,

    /// Cap per-frame gpu uploads
    #[arg(long, value_name = "bytes", display_order = 17)]
    pub gpu_bytes_per_frame: Option<usize>,
}

/// The web page's `engine_run` options: both structs as ONE flat object, which is also what the
/// engine echoes back for the url sync.
#[derive(Deserialize, Serialize, Default, Clone, PartialEq, Debug)]
pub struct EngineRunOptions {
    #[serde(flatten)]
    pub launch: LaunchOptions,
    #[serde(flatten)]
    pub client: ClientOptions,
}

impl EngineRunOptions {
    /// From the web page's options object serialised as JSON. `JSON.stringify` drops
    /// `undefined`-valued keys, which is what makes them "absent"; an unknown key fails the
    /// launch so a misspelt one can never silently fall through to a default. (Checked by hand
    /// against the web param table: serde's `flatten` and `deny_unknown_fields` are exclusive.)
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        use serde::de::Error;
        let value: serde_json::Value = serde_json::from_str(json)?;
        let Some(object) = value.as_object() else {
            return Err(Error::custom("expected an object"));
        };
        let known: Vec<String> = crate::web_params::web_params()
            .into_iter()
            .map(|p| p.name)
            .collect();
        if let Some(key) = object.keys().find(|key| !known.contains(key)) {
            return Err(Error::custom(format!(
                "unknown field `{key}`, expected one of {}",
                known.join(", ")
            )));
        }
        serde_json::from_value(value)
    }

    /// An empty string is "absent" too — the web page may pass `''` for an unset field.
    pub fn without_empty_strings(mut self) -> Self {
        for value in [
            &mut self.launch.realm,
            &mut self.launch.position,
            &mut self.launch.content_server,
            &mut self.launch.base_domain,
            &mut self.client.system_scene,
            &mut self.client.portables,
            &mut self.client.imposter_source,
        ] {
            if value.as_deref() == Some("") {
                *value = None;
            }
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        launch: LaunchOptions,
        #[command(flatten)]
        client: ClientOptions,
    }

    #[test]
    fn native_flags_are_the_web_names() {
        let cli = Cli::parse_from([
            "x",
            "--realm",
            "https://r",
            "--position",
            "1,-2",
            "--system-scene",
            "none",
            "--base-domain",
            "decentraland.zone",
            "--preview",
            "--log-fps",
            "false",
            "--gpu-bytes-per-frame",
            "500000",
        ]);
        assert_eq!(cli.launch.realm.as_deref(), Some("https://r"));
        assert_eq!(cli.launch.position.as_deref(), Some("1,-2"));
        assert_eq!(cli.client.system_scene.as_deref(), Some("none"));
        assert_eq!(cli.launch.base_domain.as_deref(), Some("decentraland.zone"));
        assert!(cli.launch.preview);
        assert!(!cli.client.editor);
        assert_eq!(cli.launch.log_fps, Some(false));
        assert_eq!(cli.client.gpu_bytes_per_frame, Some(500_000));
        // the pre-table spellings are gone, not aliased
        assert!(Cli::try_parse_from(["x", "--server", "r"]).is_err());
        assert!(Cli::try_parse_from(["x", "--ui", "none"]).is_err());
    }

    #[test]
    fn position_takes_a_negative_x() {
        // most of Genesis City is negative, and the value arrives as its own argv entry
        let cli = Cli::try_parse_from(["x", "--position", "-125,-96"]).unwrap();
        assert_eq!(cli.launch.position.as_deref(), Some("-125,-96"));
    }

    #[test]
    fn unknown_json_keys_are_rejected() {
        let err = EngineRunOptions::from_json(r#"{"pulseServr": "localhost:7777"}"#).unwrap_err();
        assert!(
            err.to_string().contains("unknown field `pulseServr`"),
            "{err}"
        );
        // the page-resolved base domain and service overrides are keys like any other
        let options = EngineRunOptions::from_json(
            r#"{"pulseServer": "localhost:7777", "preview": true, "logFps": true, "gpuBytesPerFrame": 500000, "baseDomain": "decentraland.zone", "catalyst": "http://localhost:3000"}"#,
        )
        .unwrap();
        assert_eq!(
            options.launch.base_domain.as_deref(),
            Some("decentraland.zone")
        );
        assert_eq!(
            options.launch.services.catalyst.as_deref(),
            Some("http://localhost:3000")
        );
        assert_eq!(
            options.launch.services.pulse_server.as_deref(),
            Some("localhost:7777")
        );
        assert!(options.launch.preview);
        assert!(!options.client.editor);
        assert_eq!(options.launch.log_fps, Some(true));
        assert_eq!(options.client.gpu_bytes_per_frame, Some(500_000));
        // typed keys take their type, not a string
        assert!(EngineRunOptions::from_json(r#"{"gpuBytesPerFrame": "500000"}"#).is_err());
        // a native-only service is an unknown key on web
        assert!(EngineRunOptions::from_json(r#"{"authPage": "http://localhost:1"}"#).is_err());
        // and the echo is one flat object again
        let json = serde_json::to_value(&options).unwrap();
        assert_eq!(json["pulseServer"], "localhost:7777");
        assert_eq!(json["gpuBytesPerFrame"], 500_000);
        assert_eq!(json["baseDomain"], "decentraland.zone");
        assert_eq!(json["catalyst"], "http://localhost:3000");
        assert!(
            json.get("places").is_some(),
            "every service key is echoed (as null when unset)"
        );
    }
}
