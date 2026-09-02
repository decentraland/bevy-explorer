//! The base domain every backend host is composed from; set once via --base-domain
//! (native) or the ?baseDomain= entry param (web, via boot.js + src/web.rs).

use std::sync::OnceLock;

pub const DEFAULT: &str = "decentraland.org";

static BASE_DOMAIN: OnceLock<String> = OnceLock::new();

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
