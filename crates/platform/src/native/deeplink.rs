//! `decentraland://` deep links, shared with the Decentraland launcher.
//!
//! The launcher owns the scheme wherever it is installed. When an explorer is already running
//! (or the link carries `bridgeOnly`) it does not spawn one: it writes the link to a bridge
//! file and waits for the consumer to delete it. We read that same file, so a sign-in link
//! reaches us whether the OS handed it to the launcher or to ourselves (registered on
//! windows/linux only when nothing else claims the scheme). Either way the explorer receives
//! the raw link as its first argument; a place link (`realm`, `position`, `dclenv`) becomes
//! launch options.

use std::{path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as os;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as os;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod unsupported;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
use unsupported as os;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows as os;

pub const SCHEME: &str = "decentraland";
const LAUNCHER_DIR: &str = "DecentralandLauncherLight";
const BRIDGE_FILE: &str = "deeplink-bridge.json";
const POLL_INTERVAL: Duration = Duration::from_millis(200);

#[derive(Serialize, Deserialize)]
struct BridgeFile {
    deeplink: String,
}

/// Who handles `decentraland://` links on this machine.
enum Handler {
    None,
    /// Our own registration, and the binary it points at (never on macos)
    #[allow(dead_code)]
    Ours(PathBuf),
    /// Someone else's (normally the launcher): never touched
    Other,
}

/// A place link's launch options: `decentraland://open?realm=…&position=x,y&dclenv=org|zone`.
#[derive(Default, Debug, PartialEq)]
pub struct PlaceLink {
    pub realm: Option<String>,
    pub position: Option<String>,
    pub base_domain: Option<String>,
}

/// Make sure a `decentraland://` link opened from the browser can reach us: either something
/// (normally the launcher) already handles the scheme, or we register this binary. A stale
/// registration of ours (a dev build that moved or was cleaned) is redone. On macos
/// registration needs an app bundle, so the launcher is required there.
pub fn ensure_scheme_handler() -> Result<(), anyhow::Error> {
    match os::handler() {
        Handler::Other => Ok(()),
        Handler::Ours(path) if path == os::exe_path()? => Ok(()),
        Handler::Ours(_) | Handler::None => os::register_handler(),
    }
}

/// Wait for `decentraland://open?signin=<identityId>&authRequestId=<request_id>` to land in the
/// bridge file and return the `signin` id. Links minted for another login (unity, or an
/// earlier attempt) are left in place for their owner.
pub async fn await_signin(request_id: &str, timeout: Duration) -> Result<String, anyhow::Error> {
    let start_time = std::time::Instant::now();
    loop {
        if start_time.elapsed() >= timeout {
            anyhow::bail!("timed out awaiting sign-in");
        }
        if let Some(link) = peek_launcher_bridge() {
            if let Some(identity_id) = signin_for_request(&link, request_id) {
                delete_launcher_bridge();
                return Ok(identity_id);
            }
        }
        async_std::task::sleep(POLL_INTERVAL).await;
    }
}

/// A sign-in link on the command line, i.e. the OS launched us as the scheme handler for a login
/// in progress.
pub fn signin_link_arg() -> Option<String> {
    std::env::args().skip(1).find(|arg| {
        query(arg).is_some_and(|pairs| {
            pairs
                .iter()
                .any(|(key, value)| key == "signin" && !value.is_empty())
        })
    })
}

/// Write `url` to the bridge file the way the launcher does.
pub fn write_launcher_bridge(url: &str) -> Result<(), anyhow::Error> {
    let path = launcher_bridge_path().ok_or_else(|| anyhow::anyhow!("no home directory"))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string(&BridgeFile {
        deeplink: url.to_owned(),
    })?;
    std::fs::write(path, json)?;
    Ok(())
}

/// The launch options a place link carries.
pub fn parse_place_link(link: &str) -> Result<PlaceLink, anyhow::Error> {
    let pairs = query(link).ok_or_else(|| anyhow::anyhow!("not a {SCHEME}:// link"))?;
    let mut place = PlaceLink::default();
    for (key, value) in pairs {
        match key.as_str() {
            "realm" => place.realm = Some(value),
            "position" => place.position = Some(value),
            "dclenv" => {
                place.base_domain = Some(match value.as_str() {
                    "org" => "decentraland.org".to_owned(),
                    "zone" => "decentraland.zone".to_owned(),
                    other => anyhow::bail!("unknown dclenv `{other}`"),
                })
            }
            _ => (),
        }
    }
    Ok(place)
}

/// `~/Library/Application Support/DecentralandLauncherLight/deeplink-bridge.json` on macos,
/// `%LOCALAPPDATA%\DecentralandLauncherLight\deeplink-bridge.json` on windows; the same layout
/// under `~/.local/share` on linux, where there is no launcher and only we write it.
fn launcher_bridge_path() -> Option<PathBuf> {
    directories::BaseDirs::new()
        .map(|dirs| dirs.data_local_dir().join(LAUNCHER_DIR).join(BRIDGE_FILE))
}

fn peek_launcher_bridge() -> Option<String> {
    let content = std::fs::read_to_string(launcher_bridge_path()?).ok()?;
    serde_json::from_str::<BridgeFile>(&content)
        .ok()
        .map(|file| file.deeplink)
}

fn delete_launcher_bridge() {
    if let Some(path) = launcher_bridge_path() {
        let _ = std::fs::remove_file(path);
    }
}

/// The decoded query of a `decentraland://` link, or None for anything else. Like the launcher,
/// a link without `?` (`decentraland://realm=…&position=…`) is all query.
fn query(link: &str) -> Option<Vec<(String, String)>> {
    let rest = link.strip_prefix(SCHEME)?.strip_prefix("://")?;
    let query = rest.split_once('?').map_or(rest, |(_, query)| query);
    Some(
        url::form_urlencoded::parse(query.as_bytes())
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect(),
    )
}

/// The `signin` id carried by `link`, if the link was minted for `request_id`.
fn signin_for_request(link: &str, request_id: &str) -> Option<String> {
    let mut signin = None;
    let mut auth_request_id = None;
    for (key, value) in query(link)? {
        match key.as_str() {
            "signin" => signin = Some(value),
            "authRequestId" => auth_request_id = Some(value),
            _ => (),
        }
    }
    (auth_request_id.as_deref() == Some(request_id))
        .then_some(signin)
        .flatten()
        .filter(|id| !id.is_empty())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn signin_matches_own_request_only() {
        let link = "decentraland://open?signin=id-1&bridgeOnly=true&authRequestId=req-1";
        assert_eq!(signin_for_request(link, "req-1").as_deref(), Some("id-1"));
        assert_eq!(signin_for_request(link, "req-2"), None);
        assert_eq!(
            signin_for_request("decentraland://open?signin=&authRequestId=req-1", "req-1"),
            None
        );
        assert_eq!(
            signin_for_request("https://x/?signin=id-1&authRequestId=req-1", "req-1"),
            None
        );
    }

    #[test]
    fn place_link_options() {
        assert_eq!(
            parse_place_link("decentraland://realm=http%3A%2F%2F127.0.0.1%3A8000&position=0%2C0")
                .unwrap(),
            PlaceLink {
                realm: Some("http://127.0.0.1:8000".into()),
                position: Some("0,0".into()),
                base_domain: None
            }
        );
        assert_eq!(
            parse_place_link("decentraland:///?position=139,-40&dclenv=zone").unwrap(),
            PlaceLink {
                realm: None,
                position: Some("139,-40".into()),
                base_domain: Some("decentraland.zone".into())
            }
        );
        assert!(parse_place_link("decentraland://open?dclenv=prod").is_err());
        assert!(parse_place_link("https://decentraland.org/?position=1,2").is_err());
    }
}
