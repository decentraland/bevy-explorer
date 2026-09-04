//! The entry-url parameters the web build accepts — defined ONCE, here. Exported to the react
//! page alongside the ts-rs types (scripts/gen-ts-bindings.sh writes `webParamTable.ts` into
//! react-web/src/engine/generated), so the host page reads the same table the engine is built
//! from: which params exist, what each does and how it reaches the engine. Whether a LINK may
//! set one is the front-end's policy, not the engine's (react-web lib/launchGate.ts — a
//! different host, e.g. the editor, trusts different things). `src/web_options.rs` (the
//! `engine_run` options object, which the engine also echoes back into the page url) is
//! checked against this table in a unit test.
//!
//! Native has no table: each `--flag` is parsed by hand in src/main.rs. The `doc` strings name
//! the native counterpart so this doubles as the cross-platform index.

use serde::Serialize;

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ParamKind {
    /// `?name=value`
    String,
    /// presence is the value: `?name` / `?name=true`
    Flag,
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
    /// Computed by the engine loader (engine.js) at launch — an `engine_run` option that is
    /// never read from the url.
    Page,
    /// The host page consumes it itself (composing its own backend urls) and publishes it as
    /// `window.__baseDomain()` before the wasm loads; the engine reads that at `engine_init`,
    /// ahead of `engine_run`.
    BaseDomain,
}

#[derive(Serialize, Clone, PartialEq, Eq, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WebParam {
    /// The query-string key; for everything but `BaseDomain` also the `engine_run` options key.
    pub name: &'static str,
    pub kind: ParamKind,
    pub delivery: Delivery,
    pub doc: &'static str,
}

/// The portable scene set that runs when `--portables` / `?portables=` is absent.
pub const DEFAULT_PORTABLES: &str = "basiccontroller.dcl.eth";

pub fn web_params() -> Vec<WebParam> {
    use Delivery::*;
    use ParamKind::*;
    vec![
        WebParam {
            name: "platform",
            kind: String,
            delivery: Page,
            doc: "\"macos\" | \"windows\" | \"linux\" from the user agent — picks the text-input navigation key binds.",
        },
        WebParam {
            name: "realm",
            kind: String,
            delivery: Destination,
            doc: "Realm to boot into (`--server`); absent = the persisted home realm.",
        },
        WebParam {
            name: "position",
            kind: String,
            delivery: Destination,
            doc: "Spawn parcel as `x,y` (`--location`); absent = the home parcel, or the realm's own spawn point when a realm is given.",
        },
        WebParam {
            name: "systemScene",
            kind: String,
            delivery: Launch,
            doc: "Super-user ui scene source (`--ui`), or `none` for no ui scene. The host substitutes its bundled bridge scene when absent. The engine trusts it completely: permissions.rs waves through every permission check for it and it gets the whole system api.",
        },
        WebParam {
            name: "portables",
            kind: String,
            delivery: Launch,
            doc: "`;`-separated portable/startup scene sources (`--portables`); absent = `basiccontroller.dcl.eth` (DEFAULT_PORTABLES), which the url sync also omits.",
        },
        WebParam {
            name: "preview",
            kind: Flag,
            delivery: Launch,
            doc: "Scene preview mode (`--preview`).",
        },
        WebParam {
            name: "editor",
            kind: Flag,
            delivery: Host,
            doc: "Embedded in a scene editor (`--editor`): scenes freeze after main() until the editor unfreezes them. The editor's own front-end sets it; not a link parameter.",
        },
        WebParam {
            name: "pulseServer",
            kind: String,
            delivery: Launch,
            doc: "Pulse server as `host:port` (`--pulse-server`); absent = the deployment's default.",
        },
        WebParam {
            name: "imposterSource",
            kind: String,
            delivery: Launch,
            doc: "Base url of the imposter store (`--imposter-source`); absent = the default store.",
        },
        WebParam {
            name: "sceneParams",
            kind: String,
            delivery: Page,
            doc: "The page's query string with the launch values folded in, exposed to scenes (`--params`).",
        },
        WebParam {
            name: "baseDomain",
            kind: String,
            delivery: BaseDomain,
            doc: "The deployment domain every backend host is composed from (`--base-domain`) — sign-in, content, comms, everything; absent = derived from the hosting origin, else decentraland.org.",
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes the table's VALUES next to the ts-rs types (`TS_RS_EXPORT_DIR`, set by
    /// scripts/gen-ts-bindings.sh — a no-op under a plain `cargo test`).
    #[test]
    fn export_web_param_table() {
        let Ok(dir) = std::env::var("TS_RS_EXPORT_DIR") else {
            return;
        };
        let json = serde_json::to_string_pretty(&web_params()).unwrap();
        let ts = format!(
            "// GENERATED by scripts/gen-ts-bindings.sh from crates/system_api_types/src/web_params.rs — do not edit.\n\
             import type {{ WebParam }} from './WebParam'\n\n\
             export const WEB_PARAMS: WebParam[] = {json}\n"
        );
        std::fs::write(std::path::Path::new(&dir).join("webParamTable.ts"), ts).unwrap();
    }

    #[test]
    fn names_are_unique() {
        let mut names: Vec<_> = web_params().into_iter().map(|p| p.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), web_params().len());
    }
}
