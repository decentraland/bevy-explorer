//! The options object `engine_run` takes from the page (src/web.rs) — one named key per launch
//! parameter, the web counterpart of the native command line. The engine echoes the same shape
//! back to the page as it runs (`set_url_params`) so every param given stays in the url.
//! Compiled on every target so the test below can hold it to the web param table
//! (`system_api_types::web_params`), which is what the host page is generated from.

use serde::{Deserialize, Serialize};

/// Every key is optional; absent = the engine's default. Unknown keys fail the launch (see
/// [`parse`]) so a misspelt key can never silently fall through to a default. Adding a
/// parameter = a row in the table + a field here.
#[derive(Deserialize, Serialize, Default, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct EngineRunOptions {
    pub platform: String,
    pub realm: Option<String>,
    pub position: Option<String>,
    pub system_scene: Option<String>,
    pub portables: Option<String>,
    pub preview: bool,
    pub editor: bool,
    pub scene_params: Option<String>,
    pub pulse_server: Option<String>,
    pub imposter_source: Option<String>,
}

/// From the page's object serialised as JSON. `JSON.stringify` drops `undefined`-valued keys,
/// which is what makes them "absent".
pub fn parse(json: &str) -> Result<EngineRunOptions, serde_json::Error> {
    serde_json::from_str(json)
}

/// An empty string is "absent" too — the page may pass `''` for an unset field.
pub fn non_empty(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use system_api_types::web_params::{web_params, Delivery};

    /// The struct's keys are exactly the table's engine_run params (everything but the
    /// host-consumed base domain), so the two can't drift.
    #[test]
    fn options_match_the_web_param_table() {
        let json = serde_json::to_value(EngineRunOptions::default()).unwrap();
        let fields: BTreeSet<_> = json.as_object().unwrap().keys().cloned().collect();
        let table: BTreeSet<_> = web_params()
            .into_iter()
            .filter(|p| p.delivery != Delivery::BaseDomain)
            .map(|p| p.name.to_owned())
            .collect();
        assert_eq!(fields, table);
    }

    #[test]
    fn unknown_keys_are_rejected() {
        assert!(parse(r#"{"pulseServr": "localhost:7777"}"#).is_err());
        let options = parse(r#"{"pulseServer": "localhost:7777", "preview": true}"#).unwrap();
        assert_eq!(options.pulse_server.as_deref(), Some("localhost:7777"));
        assert!(options.preview);
        assert!(!options.editor);
    }
}
