//! Entry check for worlds. A world (a `/world/<name>` realm on the configured worlds server)
//! can restrict who gets in: its `/permissions` record names an access type — unrestricted, an
//! allow-list of wallets and communities, or a shared secret (a password). The worlds server
//! enforces this at the comms handshake, so an unchecked realm change lands the player in the
//! world's scenes but outside its rooms. [`check_world_access`] runs the same check the unity
//! explorer does before the realm change is accepted, and for a password world validates the
//! password with that very handshake, so the change only goes ahead when the comms login will.

use std::time::Duration;

use bevy::prelude::*;
use common::base_domain::{self, Service};
use http::Uri;
use serde::Deserialize;
use tokio::sync::{mpsc, oneshot};
use wallet::{sign_request, SignedLoginMeta, Wallet};

/// The world's name when `realm` (a realm url, see `map_realm_name`) is a world on the configured
/// worlds server.
pub fn world_name(realm: &str) -> Option<String> {
    let worlds = base_domain::service(Service::WorldsServer);
    let name = realm
        .strip_prefix(worlds.as_str())?
        .strip_prefix("/world/")?
        .split(['/', '?', '#'])
        .next()
        .filter(|name| !name.is_empty())?;
    Some(name.to_owned())
}

/// A password the HUD must ask for; answered with the password, or `None` when the player gives
/// up. Dropped unanswered when the realm change no longer needs it.
pub struct WorldPasswordPrompt {
    pub world: String,
    /// Why the previous answer was refused; `None` on the first ask
    pub error: Option<String>,
    pub answer: oneshot::Sender<Option<String>>,
}

/// Where password prompts go, installed by whatever shows them (the system bridge). Without it a
/// password-protected world can't be entered.
#[derive(Resource, Clone)]
pub struct WorldPasswordPrompter(pub mpsc::UnboundedSender<WorldPasswordPrompt>);

/// The validated password of the world being entered, for its signed comms login; `None` on
/// any other realm.
#[derive(Resource, Default)]
pub struct WorldCommsSecret(pub Option<String>);

/// `GET /world/<name>/permissions`, the parts that decide entry.
#[derive(Deserialize, Default)]
struct WorldPermissions {
    #[serde(default)]
    owner: String,
    #[serde(default)]
    permissions: Permissions,
}

#[derive(Deserialize, Default)]
struct Permissions {
    access: Option<AccessPermission>,
}

#[derive(Deserialize, Default)]
struct AccessPermission {
    #[serde(rename = "type")]
    ty: String,
    #[serde(default)]
    wallets: Vec<String>,
    #[serde(default)]
    communities: Vec<String>,
}

/// `GET /v2/communities/<id>`, the local user's role in it.
#[derive(Deserialize)]
struct Community {
    data: CommunityData,
}

#[derive(Deserialize)]
struct CommunityData {
    role: Option<String>,
}

/// A `{"error": ...}` body.
#[derive(Deserialize)]
struct ErrorBody {
    error: String,
}

/// Whether the local user may enter `world`, and with what password. `Err` is the reason they
/// may not, in words for the player.
pub async fn check_world_access(
    realm: &str,
    world: &str,
    wallet: &Wallet,
    client: &reqwest::Client,
    prompter: Option<&WorldPasswordPrompter>,
) -> Result<Option<String>, String> {
    let worlds = base_domain::service(Service::WorldsServer);
    let permissions = client
        .get(format!("{worlds}/world/{world}/permissions"))
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .and_then(|res| res.error_for_status())
        .map_err(|e| format!("couldn't check who may enter: {e}"))?
        .json::<WorldPermissions>()
        .await
        .map_err(|e| format!("couldn't check who may enter: {e}"))?;

    let address = wallet
        .address()
        .map(|address| format!("{address:#x}"))
        .unwrap_or_default();
    let is_owner = address.eq_ignore_ascii_case(&permissions.owner);
    let access = permissions.permissions.access.unwrap_or_default();

    match access.ty.as_str() {
        "" | "unrestricted" => Ok(None),
        "allow-list" => {
            if is_owner
                || access
                    .wallets
                    .iter()
                    .any(|wallet| wallet.eq_ignore_ascii_case(&address))
            {
                return Ok(None);
            }
            for community in &access.communities {
                if is_community_member(community, wallet, client).await {
                    return Ok(None);
                }
            }
            Err("you are not on this world's access list".to_owned())
        }
        "shared-secret" => {
            if is_owner {
                return Ok(None);
            }
            let Some(prompter) = prompter else {
                return Err(
                    "this world needs a password, and there is no way to ask for one".to_owned(),
                );
            };
            let mut error = None;
            loop {
                let (answer, answered) = oneshot::channel();
                let prompt = WorldPasswordPrompt {
                    world: world.to_owned(),
                    error: error.take(),
                    answer,
                };
                let password = match prompter.0.send(prompt) {
                    Ok(()) => answered.await.ok().flatten(),
                    Err(_) => None,
                };
                let Some(password) = password else {
                    return Err("this world needs a password to enter".to_owned());
                };
                match validate_password(realm, world, &password, wallet, client).await {
                    Ok(()) => return Ok(Some(password)),
                    Err(e) => error = Some(e),
                }
            }
        }
        other => Err(format!("unsupported world access type `{other}`")),
    }
}

async fn is_community_member(community: &str, wallet: &Wallet, client: &reqwest::Client) -> bool {
    let url = format!(
        "{}/v2/communities/{community}",
        base_domain::service(Service::SocialApi)
    );
    let result: Result<Community, anyhow::Error> = async {
        let uri = Uri::try_from(&url)?;
        let headers = sign_request("get", &uri, wallet, "{}".to_owned()).await?;
        let mut request = client.get(&url).timeout(Duration::from_secs(10));
        for (key, value) in headers {
            request = request.header(key, value);
        }
        Ok(request.send().await?.error_for_status()?.json().await?)
    }
    .await;
    match result {
        Ok(community) => community.data.role.is_some_and(|role| role != "none"),
        Err(e) => {
            warn!("failed to check membership of community {community}: {e}");
            false
        }
    }
}

/// The world's comms handshake with the password: it answers 2xx only to the right one, so a
/// pass here means the comms login after the realm change will pass too.
async fn validate_password(
    realm: &str,
    world: &str,
    password: &str,
    wallet: &Wallet,
    client: &reqwest::Client,
) -> Result<(), String> {
    let worlds = base_domain::service(Service::WorldsServer);
    let uri = Uri::try_from(format!("{worlds}/worlds/{world}/comms"))
        .map_err(|e| format!("bad comms address: {e}"))?;
    let origin = Uri::try_from(realm).map_err(|e| format!("bad realm address: {e}"))?;
    let meta =
        SignedLoginMeta::new(wallet.is_guest(), origin).with_secret(Some(password.to_owned()));
    let meta = serde_json::to_string(&meta).map_err(|e| e.to_string())?;
    let headers = sign_request("post", &uri, wallet, meta)
        .await
        .map_err(|e| e.to_string())?;

    let mut request = client
        .post(uri.to_string())
        .timeout(Duration::from_secs(30));
    for (key, value) in headers {
        request = request.header(key, value);
    }
    let response = request.send().await.map_err(|e| e.to_string())?;
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    if status == reqwest::StatusCode::FORBIDDEN {
        return Err("Incorrect password".to_owned());
    }
    let body = response.text().await.unwrap_or_default();
    Err(serde_json::from_str::<ErrorBody>(&body)
        .map(|body| body.error)
        .ok()
        .filter(|error| !error.trim().is_empty())
        .unwrap_or_else(|| {
            if body.trim().is_empty() {
                format!("status: {status}")
            } else {
                body.trim().to_owned()
            }
        }))
}

#[cfg(test)]
mod tests {
    use super::world_name;

    #[test]
    fn names_a_world_on_the_worlds_server() {
        assert_eq!(
            world_name("https://worlds-content-server.decentraland.org/world/boedo.dcl.eth"),
            Some("boedo.dcl.eth".to_owned())
        );
        assert_eq!(
            world_name("https://worlds-content-server.decentraland.org/world/boedo.dcl.eth/"),
            Some("boedo.dcl.eth".to_owned())
        );
    }

    #[test]
    fn other_realms_are_not_worlds() {
        assert_eq!(
            world_name("https://realm-provider-ea.decentraland.org/main"),
            None
        );
        assert_eq!(
            world_name("https://worlds-content-server.decentraland.org/world/"),
            None
        );
        assert_eq!(
            world_name("https://worlds-content-server.decentraland.zone/world/boedo.dcl.eth"),
            None
        );
    }
}
