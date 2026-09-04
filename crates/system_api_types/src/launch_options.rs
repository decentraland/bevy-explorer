//! The launch parameters both platforms accept — defined ONCE, here. Natively each field is a
//! `--flag` (clap; the doc comment is its `--help` text), on web the same field is a key of the
//! `engine_run` options object (serde, camelCase) and an entry-url query param. The web param
//! table (`web_params`) is derived from this struct's clap metadata plus a per-field delivery
//! annotation, and the host page is generated from that table — so a parameter is declared
//! here and nowhere else.
//!
//! Field names are the web names, and the native flags are the same names in kebab-case.
//! Everything is optional: absent = the engine's default. Native-only flags live on `DecentralandArguments` (src/lib.rs), which
//! flattens this struct in.

use serde::{Deserialize, Serialize};

#[derive(clap::Args, Deserialize, Serialize, Default, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct LaunchOptions {
    /// Realm to boot into; absent = the persisted home realm or default realm
    #[arg(long, value_name = "url", display_order = 1)]
    pub realm: Option<String>,

    /// Spawn parcel as `x,y`; absent = the home parcel, or the realm's spawn point
    #[arg(long, value_name = "x,y", display_order = 2)]
    pub position: Option<String>,

    /// Scene preview mode: hot-reloading, no failed-asset backoff, plain-http fetches allowed,
    /// realm fixed.
    #[arg(long, display_order = 3)]
    pub preview: bool,

    /// The deployment domain every backend host is composed from — sign-in, content, comms,
    /// everything; absent = decentraland.org. on web: derived from the hosting origin.
    #[serde(skip)]
    #[arg(long, value_name = "domain", display_order = 4)]
    pub base_domain: Option<String>,

    /// Embedded in a scene editor (creator hub). Set by editor front-ends.
    #[arg(long, display_order = 5)]
    pub editor: bool,

    /// Pulse server as `host:port`; absent = the deployment's default
    #[arg(long, value_name = "host:port", display_order = 8)]
    pub pulse_server: Option<String>,

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
}

impl LaunchOptions {
    /// From the web page's options object serialised as JSON. `JSON.stringify` drops
    /// `undefined`-valued keys, which is what makes them "absent"; an unknown key fails the
    /// launch so a misspelt one can never silently fall through to a default.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// An empty string is "absent" too — the web page may pass `''` for an unset field.
    pub fn without_empty_strings(mut self) -> Self {
        for value in [
            &mut self.realm,
            &mut self.position,
            &mut self.system_scene,
            &mut self.portables,
            &mut self.pulse_server,
            &mut self.imposter_source,
            &mut self.base_domain,
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
        ]);
        assert_eq!(cli.launch.realm.as_deref(), Some("https://r"));
        assert_eq!(cli.launch.position.as_deref(), Some("1,-2"));
        assert_eq!(cli.launch.system_scene.as_deref(), Some("none"));
        assert_eq!(cli.launch.base_domain.as_deref(), Some("decentraland.zone"));
        assert!(cli.launch.preview);
        assert!(!cli.launch.editor);
        // the pre-table spellings are gone, not aliased
        assert!(Cli::try_parse_from(["x", "--server", "r"]).is_err());
        assert!(Cli::try_parse_from(["x", "--ui", "none"]).is_err());
    }

    #[test]
    fn unknown_json_keys_are_rejected() {
        assert!(LaunchOptions::from_json(r#"{"pulseServr": "localhost:7777"}"#).is_err());
        // the base domain never travels as an engine_run key
        assert!(LaunchOptions::from_json(r#"{"baseDomain": "decentraland.zone"}"#).is_err());
        let options =
            LaunchOptions::from_json(r#"{"pulseServer": "localhost:7777", "preview": true}"#)
                .unwrap();
        assert_eq!(options.pulse_server.as_deref(), Some("localhost:7777"));
        assert!(options.preview);
        assert!(!options.editor);
    }
}
