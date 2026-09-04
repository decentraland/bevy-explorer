//! The backend services the engine and the react HUD talk to, and the launch parameter that
//! points each one somewhere else. By deployment convention every service lives at
//! `{scheme}://{sub}.{base domain}{path}`; an override is a FULL base url that replaces that
//! composition for one service (a local instance, a mixed deployment) while the rest keep
//! following the domain. Resolution — override, else composition — is
//! `common::base_domain::service`; on web the HUD resolves the same way from the same table
//! (react-web lib/baseDomain.ts, from the generated `serviceTable.ts`) because it composes its
//! own urls before the engine exists.

use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, EnumIter)]
pub enum Service {
    /// The realm provider: its `/main` is the realm to boot into when none is given or persisted
    RealmProvider,
    /// The catalyst peer: `/content` and `/lambdas`
    Catalyst,
    /// Worlds content server: `/world/<name>` realms
    WorldsServer,
    /// Places api: place and world metadata
    Places,
    /// Comms gatekeeper: LiveKit adapters for scenes
    CommsGatekeeper,
    /// The local-preview comms gatekeeper
    PreviewGatekeeper,
    /// Auth api the browser sign-in flow polls
    AuthApi,
    /// The sign-in web page the browser is sent to
    AuthPage,
    /// Asset-bundle registry: profile lookups
    AssetBundleRegistry,
    /// World storage (storage delegation signing)
    Storage,
    /// Ethereum json-rpc websocket
    EthereumRpc,
    /// Social service websocket (friends)
    SocialRpc,
    /// OpenSea api (NftShape metadata)
    Opensea,
    /// Reels (camera reel photos) — HUD only
    Reels,
    /// Map tile api — HUD only
    MapApi,
}

impl Service {
    /// The deployment-convention default as `(scheme, sub, path)`; an empty sub is the apex.
    pub const fn composition(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Service::RealmProvider => ("https", "realm-provider-ea", ""),
            Service::Catalyst => ("https", "peer", ""),
            Service::WorldsServer => ("https", "worlds-content-server", ""),
            Service::Places => ("https", "places", ""),
            Service::CommsGatekeeper => ("https", "comms-gatekeeper", ""),
            Service::PreviewGatekeeper => ("https", "comms-gatekeeper-local", ""),
            Service::AuthApi => ("https", "auth-api", ""),
            Service::AuthPage => ("https", "", "/auth"),
            Service::AssetBundleRegistry => ("https", "asset-bundle-registry", ""),
            Service::Storage => ("https", "storage", ""),
            Service::EthereumRpc => ("wss", "rpc", ""),
            Service::SocialRpc => ("wss", "rpc-social-service-ea", ""),
            Service::Opensea => ("https", "opensea", ""),
            Service::Reels => ("https", "reels", ""),
            Service::MapApi => ("https", "api", ""),
        }
    }

    /// The [`ServiceOverrides`] field (= the native flag in kebab-case, the web param in
    /// camelCase) that overrides it.
    pub const fn field(self) -> &'static str {
        match self {
            Service::RealmProvider => "realm_provider",
            Service::Catalyst => "catalyst",
            Service::WorldsServer => "worlds_server",
            Service::Places => "places",
            Service::CommsGatekeeper => "comms_gatekeeper",
            Service::PreviewGatekeeper => "preview_gatekeeper",
            Service::AuthApi => "auth_api",
            Service::AuthPage => "auth_page",
            Service::AssetBundleRegistry => "asset_bundle_registry",
            Service::Storage => "storage",
            Service::EthereumRpc => "ethereum_rpc",
            Service::SocialRpc => "social_rpc",
            Service::Opensea => "opensea",
            Service::Reels => "reels",
            Service::MapApi => "map_api",
        }
    }

    /// Every service, in declaration order.
    pub fn all() -> impl Iterator<Item = Service> {
        use strum::IntoEnumIterator;
        Service::iter()
    }

    /// Whether the web build can point the service elsewhere. The web page signs in at its own
    /// origin's `/auth` and hands the engine the identity, so the browser sign-in services are
    /// not the wasm's to redirect: they have no web param and no row in the HUD's table.
    pub const fn has_web_param(self) -> bool {
        !matches!(self, Service::AuthApi | Service::AuthPage)
    }

    pub fn flag(self) -> String {
        format!("--{}", self.field().replace('_', "-"))
    }

    pub fn param(self) -> String {
        crate::web_params::camel_case(self.field())
    }

    /// Whether the service is one of the secure-websocket ones (the override must be `ws(s)://`
    /// rather than `http(s)://`).
    pub fn is_websocket(self) -> bool {
        self.composition().0 == "wss"
    }
}

/// One full base url per service, each replacing that service's base-domain composition. A
/// value is taken verbatim with paths appended, so it carries its own scheme and port and no
/// trailing slash (`http://localhost:3000`).
// per-arg rather than the struct's `next_help_heading`: clap_derive doesn't restore the parent's
// heading after a flatten, so a struct-level one would capture every flag a binary declares after
// this struct
const HELP_HEADING: &str =
    "Service endpoints (full base urls; absent = composed from the base domain)";

#[derive(clap::Args, Deserialize, Serialize, Default, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct ServiceOverrides {
    /// Realm provider (the realm list, and `/main` is the default realm); absent =
    /// `https://realm-provider-ea.<base>`
    #[arg(long, value_name = "url", display_order = 61, help_heading = HELP_HEADING)]
    pub realm_provider: Option<String>,

    /// Catalyst peer (`/content`, `/lambdas`); absent = `https://peer.<base>`
    #[arg(long, value_name = "url", display_order = 62, help_heading = HELP_HEADING)]
    pub catalyst: Option<String>,

    /// Worlds content server; absent = `https://worlds-content-server.<base>`
    #[arg(long, value_name = "url", display_order = 63, help_heading = HELP_HEADING)]
    pub worlds_server: Option<String>,

    /// Places api; absent = `https://places.<base>`
    #[arg(long, value_name = "url", display_order = 64, help_heading = HELP_HEADING)]
    pub places: Option<String>,

    /// Comms gatekeeper; absent = `https://comms-gatekeeper.<base>`
    #[arg(long, value_name = "url", display_order = 65, help_heading = HELP_HEADING)]
    pub comms_gatekeeper: Option<String>,

    /// Local-preview comms gatekeeper; absent = `https://comms-gatekeeper-local.<base>`
    #[arg(long, value_name = "url", display_order = 66, help_heading = HELP_HEADING)]
    pub preview_gatekeeper: Option<String>,

    /// Auth api the sign-in flow polls (native only); absent = `https://auth-api.<base>`
    #[arg(long, value_name = "url", display_order = 67, help_heading = HELP_HEADING)]
    pub auth_api: Option<String>,

    /// Sign-in page the browser opens (native only); absent = `https://<base>/auth`
    #[arg(long, value_name = "url", display_order = 68, help_heading = HELP_HEADING)]
    pub auth_page: Option<String>,

    /// Asset-bundle registry; absent = `https://asset-bundle-registry.<base>` (custom domains
    /// only: org and zone profiles use the registry of the profile's own environment)
    #[arg(long, value_name = "url", display_order = 69, help_heading = HELP_HEADING)]
    pub asset_bundle_registry: Option<String>,

    /// World storage; absent = `https://storage.<base>`. https only: an http instance is never
    /// signed for (the delegation claim must not go out in cleartext)
    #[arg(long, value_name = "url", display_order = 70, help_heading = HELP_HEADING)]
    pub storage: Option<String>,

    /// Ethereum json-rpc websocket; absent = `wss://rpc.<base>`
    #[arg(long, value_name = "url", display_order = 71, help_heading = HELP_HEADING)]
    pub ethereum_rpc: Option<String>,

    /// Social service websocket; absent = `wss://rpc-social-service-ea.<base>`
    #[arg(long, value_name = "url", display_order = 72, help_heading = HELP_HEADING)]
    pub social_rpc: Option<String>,

    /// OpenSea api; absent = `https://opensea.<base>`
    #[arg(long, value_name = "url", display_order = 73, help_heading = HELP_HEADING)]
    pub opensea: Option<String>,

    /// Reels (camera reel) api, used by the HUD; absent = `https://reels.<base>`
    #[arg(long, value_name = "url", display_order = 74, help_heading = HELP_HEADING)]
    pub reels: Option<String>,

    /// Map tile api, used by the HUD; absent = `https://api.<base>`
    #[arg(long, value_name = "url", display_order = 75, help_heading = HELP_HEADING)]
    pub map_api: Option<String>,
}

impl ServiceOverrides {
    pub fn get(&self, service: Service) -> Option<&str> {
        match service {
            Service::RealmProvider => &self.realm_provider,
            Service::Catalyst => &self.catalyst,
            Service::WorldsServer => &self.worlds_server,
            Service::Places => &self.places,
            Service::CommsGatekeeper => &self.comms_gatekeeper,
            Service::PreviewGatekeeper => &self.preview_gatekeeper,
            Service::AuthApi => &self.auth_api,
            Service::AuthPage => &self.auth_page,
            Service::AssetBundleRegistry => &self.asset_bundle_registry,
            Service::Storage => &self.storage,
            Service::EthereumRpc => &self.ethereum_rpc,
            Service::SocialRpc => &self.social_rpc,
            Service::Opensea => &self.opensea,
            Service::Reels => &self.reels,
            Service::MapApi => &self.map_api,
        }
        .as_deref()
        .filter(|url| !url.is_empty())
    }

    /// The overrides that were given.
    pub fn iter(&self) -> impl Iterator<Item = (Service, &str)> {
        Service::all().filter_map(|service| self.get(service).map(|url| (service, url)))
    }
}

/// A row of the service table exported to the react HUD: which web param overrides the service
/// and how its default composes, so the HUD resolves the urls it needs exactly as the engine does.
/// Only the services with a web param ([`Service::has_web_param`]).
#[derive(Serialize, Clone, PartialEq, Eq, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ServiceDef {
    /// The web param (and `ServiceOverrides` field in camelCase)
    pub name: String,
    pub scheme: String,
    /// Empty = the apex domain
    pub sub: String,
    pub path: String,
}

pub fn service_table() -> Vec<ServiceDef> {
    Service::all()
        .filter(|service| service.has_web_param())
        .map(|service| {
            let (scheme, sub, path) = service.composition();
            ServiceDef {
                name: service.param(),
                scheme: scheme.to_owned(),
                sub: sub.to_owned(),
                path: path.to_owned(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Args as _;
    use std::collections::BTreeSet;

    /// Writes the service table next to the ts-rs types (`TS_RS_EXPORT_DIR`, set by
    /// scripts/gen-ts-bindings.sh — a no-op under a plain `cargo test`).
    #[test]
    fn export_service_table() {
        let Ok(dir) = std::env::var("TS_RS_EXPORT_DIR") else {
            return;
        };
        let json = serde_json::to_string_pretty(&service_table()).unwrap();
        let ts = format!(
            "// GENERATED by scripts/gen-ts-bindings.sh from crates/system_api_types/src/services.rs — do not edit.\n\
             import type {{ ServiceDef }} from './ServiceDef'\n\n\
             export const SERVICES: ServiceDef[] = {json}\n"
        );
        std::fs::write(std::path::Path::new(&dir).join("serviceTable.ts"), ts).unwrap();
    }

    /// Every variant's field is a real flag of the struct, and every flag is some variant's.
    #[test]
    fn fields_match_the_variants() {
        let flags: BTreeSet<_> = ServiceOverrides::augment_args(clap::Command::new("x"))
            .get_arguments()
            .map(|a| a.get_id().as_str().to_owned())
            .collect();
        let fields: BTreeSet<_> = Service::all().map(|s| s.field().to_owned()).collect();
        assert_eq!(flags, fields);
        let overrides = ServiceOverrides {
            catalyst: Some("http://localhost:3000".into()),
            reels: Some(String::new()),
            ..Default::default()
        };
        assert_eq!(
            overrides.iter().collect::<Vec<_>>(),
            vec![(Service::Catalyst, "http://localhost:3000")]
        );
        assert_eq!(Service::WorldsServer.flag(), "--worlds-server");
        assert_eq!(Service::WorldsServer.param(), "worldsServer");
    }

    /// The sign-in services are native flags only: no row in the HUD's table.
    #[test]
    fn auth_services_have_no_web_side() {
        let table: BTreeSet<_> = service_table().into_iter().map(|d| d.name).collect();
        assert!(!table.contains("authApi") && !table.contains("authPage"));
        assert!(table.contains("catalyst"));
    }
}
