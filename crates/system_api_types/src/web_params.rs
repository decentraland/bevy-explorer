//! The entry-url parameters the web build accepts, as the react page sees them. Derived from
//! [`LaunchOptions`] + [`ClientOptions`] (the one declaration of every launch parameter, native
//! and web): each of their clap args becomes a row — camelCase name, kind from whether the flag takes a value, doc
//! from the `--help` text — joined with the web-only fact this module owns, how the value gets
//! from the entry url to the engine ([`Delivery`]). Exported to the react page alongside the
//! ts-rs types (scripts/gen-ts-bindings.sh writes `webParamTable.ts` into
//! react-web/src/engine/generated), so the host page reads the same table the engine is built
//! from. Whether a LINK may set a param is the front-end's policy, not the engine's (react-web
//! lib/launchGate.ts — a different host, e.g. the editor, trusts different things).

use std::any::TypeId;

use clap::Args;
use serde::Serialize;

use crate::{
    launch_options::{ClientOptions, LaunchOptions},
    services::Service,
};

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ParamKind {
    /// `?name=value`, passed as a string
    String,
    /// presence is the value: `?name` / `?name=true`
    Flag,
    /// `?name=true|false`, passed as a boolean
    Bool,
    /// `?name=<integer>`, passed as a number
    Number,
}

/// How the value gets from the entry url to the engine.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Delivery {
    /// Read from the entry url by the host page into `window.__bevyBootConfig`, which boot.js
    /// forwards verbatim as `engine_run` options.
    Launch,
    /// Chosen by the host's destination picker (a `?realm=`/`?position=` link only seeds it) and
    /// passed to `__bevyLaunch(realm, position)`; an `engine_run` option like `Launch`.
    Destination,
    /// Set by an embedding host page from its own knowledge (the creator-hub editor sets
    /// `editor`) — an `engine_run` option the react page must NOT read from the entry url.
    Host,
    /// Read from the entry url by the host page, which resolves it for its own use before the
    /// engine exists (the base domain: normalised, else derived from the hosting origin; a
    /// service url: validated) and passes the resolved value as an `engine_run` option like
    /// `Launch`. The engine never reads these from the page by any other route.
    Resolved,
}

#[derive(Serialize, Clone, PartialEq, Eq, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WebParam {
    /// The query-string key, and the `engine_run` options key.
    pub name: String,
    pub kind: ParamKind,
    pub delivery: Delivery,
    pub doc: String,
}

/// The portable scene set that runs when `--portables` / `?portables=` is absent.
pub const DEFAULT_PORTABLES: &str = "basiccontroller.dcl.eth";

/// The web-only half of a parameter's declaration, keyed by the [`LaunchOptions`] field.
fn delivery(field: &str) -> Delivery {
    use Delivery::*;
    if Service::all().any(|s| s.field() == field) {
        return Resolved;
    }
    match field {
        "realm" | "position" => Destination,
        "system_scene"
        | "portables"
        | "preview"
        | "imposter_source"
        | "content_server"
        | "log_fps"
        | "gpu_bytes_per_frame" => Launch,
        "editor" => Host,
        "base_domain" => Resolved,
        other => {
            panic!("launch option `{other}` has no web delivery — add it to web_params::delivery")
        }
    }
}

pub(crate) fn camel_case(snake: &str) -> String {
    let mut out = String::with_capacity(snake.len());
    let mut upper = false;
    for c in snake.chars() {
        if c == '_' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// The table, in the native `--help`'s order: the unheaded destination flags, the service
/// endpoints, then the rest — each run in declaration order.
pub fn web_params() -> Vec<WebParam> {
    use crate::launch_options::help_heading::SERVICES;
    let launch = LaunchOptions::augment_args(clap::Command::new("launch"));
    let client = ClientOptions::augment_args(clap::Command::new("client"));
    let native_only =
        |field: &str| Service::all().any(|s| s.field() == field && !s.has_web_param());
    let section = |arg: &clap::Arg| match arg.get_help_heading() {
        None => 0,
        Some(SERVICES) => 1,
        Some(_) => 2,
    };
    let mut args: Vec<_> = launch
        .get_arguments()
        .chain(client.get_arguments())
        .collect();
    args.sort_by_key(|arg| section(arg));
    args.into_iter()
        .filter(|arg| !native_only(arg.get_id().as_str()))
        .map(|arg| {
            let field = arg.get_id().as_str();
            WebParam {
                name: camel_case(field),
                kind: if !arg.get_action().takes_values() {
                    ParamKind::Flag
                } else if arg.get_value_parser().type_id() == TypeId::of::<bool>() {
                    ParamKind::Bool
                } else if arg.get_value_parser().type_id() == TypeId::of::<usize>() {
                    ParamKind::Number
                } else {
                    ParamKind::String
                },
                delivery: delivery(field),
                doc: arg
                    .get_long_help()
                    .or(arg.get_help())
                    .map(ToString::to_string)
                    .unwrap_or_default(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch_options::EngineRunOptions;
    use std::collections::BTreeSet;

    /// Writes the table's VALUES next to the ts-rs types (`TS_RS_EXPORT_DIR`, set by
    /// scripts/gen-ts-bindings.sh — a no-op under a plain `cargo test`).
    #[test]
    fn export_web_param_table() {
        let Ok(dir) = std::env::var("TS_RS_EXPORT_DIR") else {
            return;
        };
        let json = serde_json::to_string_pretty(&web_params()).unwrap();
        let ts = format!(
            "// GENERATED by scripts/gen-ts-bindings.sh from crates/system_api_types/src/launch_options.rs — do not edit.\n\
             import type {{ WebParam }} from './WebParam'\n\n\
             export const WEB_PARAMS: WebParam[] = {json}\n"
        );
        std::fs::write(std::path::Path::new(&dir).join("webParamTable.ts"), ts).unwrap();
    }

    /// Every field has a delivery (the panic in `delivery`), every row has a doc, and the
    /// `engine_run` keys — the structs as serialised, one flat object — are exactly the table
    /// (less the native-only services, which are no web param).
    #[test]
    fn table_matches_the_struct() {
        let params = web_params();
        for p in &params {
            assert!(!p.doc.is_empty(), "{} has no doc comment", p.name);
        }
        let names: BTreeSet<_> = params.iter().map(|p| p.name.clone()).collect();
        assert_eq!(names.len(), params.len(), "duplicate names");
        let json = serde_json::to_value(EngineRunOptions::default()).unwrap();
        let native_only: BTreeSet<_> = Service::all()
            .filter(|s| !s.has_web_param())
            .map(Service::param)
            .collect();
        let fields: BTreeSet<_> = json
            .as_object()
            .unwrap()
            .keys()
            .filter(|k| !native_only.contains(*k))
            .cloned()
            .collect();
        assert_eq!(fields, names);
        let delivery = |name: &str| params.iter().find(|p| p.name == name).unwrap().delivery;
        assert_eq!(delivery("baseDomain"), Delivery::Resolved);
        let catalyst = params.iter().find(|p| p.name == "catalyst").unwrap();
        assert_eq!(catalyst.delivery, Delivery::Resolved);
        assert_eq!(catalyst.kind, ParamKind::String);
        // the native-only sign-in services are no web param at all
        assert!(!names.contains("authApi") && !names.contains("authPage"));
        assert_eq!(
            params.iter().find(|p| p.name == "preview").unwrap().kind,
            ParamKind::Flag
        );
        let kind = |name: &str| params.iter().find(|p| p.name == name).unwrap().kind;
        assert_eq!(kind("systemScene"), ParamKind::String);
        assert_eq!(kind("logFps"), ParamKind::Bool);
        assert_eq!(kind("gpuBytesPerFrame"), ParamKind::Number);
    }
}
