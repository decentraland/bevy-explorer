//! The base domain every backend host is composed from; set once from the shared launch options
//! (`--base-domain` / `?baseDomain=`, by src/launch.rs `latch` on every binary). On top of it the
//! per-[`Service`] resolver: an explicit override (`--<service> <url>` / `?<service>=`, latched
//! once with [`set_services`]) wins, else the service composes from the domain by convention.
//! Service urls go through [`service`] / [`url`] (an authority service — pulse — through
//! [`with_default_port`]).

use std::{collections::HashMap, sync::OnceLock};

pub use system_api_types::services::{Service, ServiceValue};

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

fn host(sub: &str) -> String {
    format!("{sub}.{}", get())
}

/// Latch the per-service overrides, once, before anything composes a service url. Each value
/// takes the service's shape ([`Service::value`]): a full base url whose scheme fits the service
/// (http(s) or ws(s)), trailing slash dropped so paths can always be appended — or, for an
/// authority service, `host` or `host:port`.
pub fn set_services<'a>(
    overrides: impl IntoIterator<Item = (Service, &'a str)>,
) -> Result<(), String> {
    let mut map = HashMap::new();
    for (service, value) in overrides {
        let flag = service.flag();
        let normalised = match service.value() {
            ServiceValue::Authority => {
                let (host, port) =
                    split_authority(value).map_err(|e| format!("{flag} {value}: {e}"))?;
                match port {
                    Some(port) => format!("{host}:{port}"),
                    None => host,
                }
            }
            kind => {
                let parsed = url::Url::parse(value.trim())
                    .map_err(|e| format!("{flag} {value}: not a url ({e})"))?;
                let (scheme_ok, expected) = match kind {
                    ServiceValue::Websocket => (matches!(parsed.scheme(), "ws" | "wss"), "ws(s)"),
                    _ => (matches!(parsed.scheme(), "http" | "https"), "http(s)"),
                };
                if !scheme_ok || parsed.host_str().is_none() {
                    return Err(format!(
                        "{flag} {value}: must be a full {expected} base url"
                    ));
                }
                if parsed.query().is_some() || parsed.fragment().is_some() {
                    return Err(format!(
                        "{flag} {value}: a base url takes no query or fragment"
                    ));
                }
                parsed.as_str().trim_end_matches('/').to_owned()
            }
        };
        map.insert(service, normalised);
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

/// The service's base url (an authority service: its host) — its override, else its
/// composition from the base domain.
pub fn service(service: Service) -> String {
    if let Some(value) = service_override(service) {
        return value.to_owned();
    }
    let (scheme, sub, path) = service.composition();
    let host = if sub.is_empty() {
        get().to_owned()
    } else {
        self::host(sub)
    };
    if scheme.is_empty() {
        host
    } else {
        format!("{scheme}://{host}{path}")
    }
}

/// `host` or `host:port` → (lowercased host, the port if given). Nothing else: no scheme,
/// path, query, fragment or userinfo.
fn split_authority(authority: &str) -> Result<(String, Option<u16>), String> {
    let a = authority.trim();
    if a.is_empty() || a.contains(['/', '?', '#', '@']) {
        return Err("must be `host` or `host:port`".to_owned());
    }
    // behind a non-special scheme, so a default-looking port is kept as given
    let parsed = url::Url::parse(&format!("dummy://{a}"))
        .map_err(|e| format!("not a `host` or `host:port` ({e})"))?;
    match parsed.host_str() {
        Some(host) if !host.is_empty() => Ok((host.to_lowercase(), parsed.port())),
        _ => Err("must be `host` or `host:port`".to_owned()),
    }
}

/// An authority endpoint as (host, port): `authority` is `host` or `host:port` — an override, an
/// env var, or the composed [`service`] host — and `default_port` fills in a missing port.
pub fn with_default_port(authority: &str, default_port: u16) -> Result<(String, u16), String> {
    let (host, port) = split_authority(authority)?;
    Ok((host, port.unwrap_or(default_port)))
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
        assert_eq!(
            service(Service::PulseServer),
            "pulse-server.decentraland.org"
        );
    }

    #[test]
    fn authority_defaults_the_port_only() {
        let ok = |a: &str| with_default_port(a, 7777).unwrap();
        assert_eq!(
            ok("pulse-server.decentraland.org"),
            ("pulse-server.decentraland.org".to_owned(), 7777)
        );
        assert_eq!(ok(" Local:1234 "), ("local".to_owned(), 1234));
        assert_eq!(ok("127.0.0.1:80"), ("127.0.0.1".to_owned(), 80));
        assert_eq!(ok("[::1]"), ("[::1]".to_owned(), 7777));
        for bad in [
            "",
            "https://h",
            "h:1/x",
            "h:99999",
            "user@h",
            "h?x",
            ":7777",
        ] {
            assert!(with_default_port(bad, 7777).is_err(), "`{bad}`");
        }
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
            (Service::PulseServer, "https://pulse.example"),
            (Service::PulseServer, "pulse.example:7777/x"),
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
