//! The base domain every backend host is composed from; set once from the shared launch options
//! (`--base-domain` / `?baseDomain=`, by src/launch.rs `latch` on every binary). On top of it the
//! per-[`Service`] resolver: an explicit override (`--<service> <url>` / `?<service>=`, latched
//! once with [`set_services`]) wins, else the service composes from the domain by convention.
//! Service urls should go through [`service`] / [`url`]; the raw [`https`] / [`wss`] / [`host`]
//! composition stays for the odd host that is not a service (pulse's `host:port`).

use std::{collections::HashMap, sync::OnceLock};

pub use system_api_types::services::Service;

pub const DEFAULT: &str = "decentraland.org";

static BASE_DOMAIN: OnceLock<String> = OnceLock::new();
static SERVICES: OnceLock<HashMap<Service, String>> = OnceLock::new();

pub fn set(domain: &str) -> Result<(), String> {
    let d = domain.trim().to_ascii_lowercase();
    // ascii labels only: the value is spliced into http::Uri authorities, which reject
    // anything else (unwrapped at the composition sites, so a bad value must stop here)
    let label_ok =
        |l: &str| !l.is_empty() && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
    if !d.contains('.') || !d.split('.').all(label_ok) {
        return Err(format!(
            "--base-domain must be a bare ascii domain (no scheme, path or port): `{d}`"
        ));
    }
    if BASE_DOMAIN.set(d.clone()).is_err() && get() != d {
        return Err(format!(
            "base domain already resolved to `{}`; `--base-domain {d}` came too late",
            get()
        ));
    }
    Ok(())
}

pub fn get() -> &'static str {
    BASE_DOMAIN.get().map(String::as_str).unwrap_or(DEFAULT)
}

pub fn is_custom() -> bool {
    !matches!(get(), DEFAULT | "decentraland.zone")
}

pub fn https(sub: &str, path: &str) -> String {
    format!("https://{}{path}", host(sub))
}

pub fn wss(sub: &str, path: &str) -> String {
    format!("wss://{}{path}", host(sub))
}

pub fn host(sub: &str) -> String {
    format!("{sub}.{}", get())
}

/// Latch the per-service overrides, once, before anything composes a service url. Each value
/// is a full base url: its scheme must fit the service (http(s) or ws(s)), and a trailing slash
/// is dropped so paths can always be appended.
pub fn set_services<'a>(
    overrides: impl IntoIterator<Item = (Service, &'a str)>,
) -> Result<(), String> {
    let mut map = HashMap::new();
    for (service, url) in overrides {
        let flag = service.flag();
        let parsed =
            url::Url::parse(url.trim()).map_err(|e| format!("{flag} {url}: not a url ({e})"))?;
        let scheme_ok = if service.is_websocket() {
            matches!(parsed.scheme(), "ws" | "wss")
        } else {
            matches!(parsed.scheme(), "http" | "https")
        };
        if !scheme_ok || parsed.host_str().is_none() {
            return Err(format!(
                "{flag} {url}: must be a full {} base url",
                if service.is_websocket() {
                    "ws(s)"
                } else {
                    "http(s)"
                }
            ));
        }
        if parsed.query().is_some() || parsed.fragment().is_some() {
            return Err(format!(
                "{flag} {url}: a base url takes no query or fragment"
            ));
        }
        map.insert(service, parsed.as_str().trim_end_matches('/').to_owned());
    }
    // nothing given latches nothing: `latch` runs unconditionally, and no overrides means the
    // lock stays empty (every `service_override` None) rather than holding an empty map
    if map.is_empty() {
        return Ok(());
    }
    // re-latching the same set is fine, like `set` (the lock is process-wide: a crate's tests
    // share it)
    if SERVICES.set(map.clone()).is_err() && SERVICES.get() != Some(&map) {
        return Err("service overrides already latched with different values".to_owned());
    }
    Ok(())
}

/// The explicit override for a service, if one was given.
pub fn service_override(service: Service) -> Option<&'static str> {
    SERVICES.get()?.get(&service).map(String::as_str)
}

/// The service's base url: its override, else its composition from the base domain.
pub fn service(service: Service) -> String {
    if let Some(url) = service_override(service) {
        return url.to_owned();
    }
    let (scheme, sub, path) = service.composition();
    if sub.is_empty() {
        format!("{scheme}://{}{path}", get())
    } else {
        format!("{scheme}://{}{path}", host(sub))
    }
}

/// A url under a service: its base url with `path` (leading `/`) appended.
pub fn url(service: Service, path: &str) -> String {
    format!("{}{path}", self::service(service))
}

#[cfg(test)]
mod tests {
    use super::*;

    // NOTE: asserts literal URLs, so no test in this crate may set() a custom domain —
    // tests share the process-wide OnceLock.
    #[test]
    fn default_composition_targets_decentraland() {
        assert_eq!(
            https("auth-api", "/requests"),
            "https://auth-api.decentraland.org/requests"
        );
        assert_eq!(
            wss("rpc-social-service-ea", ""),
            "wss://rpc-social-service-ea.decentraland.org"
        );
        assert_eq!(host("pulse-server"), "pulse-server.decentraland.org");
        assert_eq!(
            url(Service::RealmProvider, "/main"),
            "https://realm-provider-ea.decentraland.org/main"
        );
        assert_eq!(
            url(Service::AuthPage, "/requests"),
            "https://decentraland.org/auth/requests"
        );
        assert_eq!(
            url(Service::SocialRpc, ""),
            "wss://rpc-social-service-ea.decentraland.org"
        );
    }

    // NOTE: never latches (the OnceLock is process-wide); only the rejections are testable here.
    #[test]
    fn rejects_bad_service_overrides() {
        for (service, bad) in [
            (Service::Catalyst, "localhost:3000"),
            (Service::Catalyst, "wss://peer.example"),
            (Service::SocialRpc, "https://social.example"),
            (Service::Places, "https://places.example/?x=1"),
            (Service::Places, ""),
        ] {
            assert!(
                set_services([(service, bad)]).is_err(),
                "`{bad}` should be refused for {service:?}"
            );
        }
    }

    #[test]
    fn rejects_non_bare_domains() {
        for bad in [
            "",
            "https://interconnected.online",
            "interconnected.online/",
            "interconnected.online/path",
            "interconnected.online:443",
            "dcl one",
            "localhost",
            ".interconnected.online",
            "interconnected.online.",
            "interconnected..online",
            "münchen.de",
        ] {
            assert!(set(bad).is_err(), "`{bad}` should be refused");
        }
    }
}
