//! Entry check for worlds. A world (a `/world/<name>` realm on the configured worlds server)
//! can restrict who gets in: its `/permissions` record names an access type — unrestricted, an
//! allow-list of wallets and communities, or a shared secret (a password). The worlds server
//! enforces this at the comms handshake, so an unchecked realm change lands the player in the
//! world's scenes but outside its rooms. [`check_world_access`] runs the same check the unity
//! explorer does before the realm change is accepted, and for a password world validates the
//! password with that very handshake, so the change only goes ahead when the comms login will.

use std::time::Duration;

use bevy::prelude::*;
use common::{
    base_domain::{self, Service},
    structs::{WorldPasswordPrompt, WorldPasswordPrompter},
};
use http::Uri;
use serde::Deserialize;
use tokio::sync::{oneshot, watch};
use wallet::{sign_request, SignedLoginMeta, Wallet};

pub const SUPERSEDED: &str = "superseded by a later realm change";

/// Whether a later realm change has been issued since the one this belongs to, which then stands
/// down rather than finish behind it.
pub struct Superseded {
    pub latest: watch::Receiver<u64>,
    pub id: u64,
}

impl Superseded {
    pub fn is_superseded(&self) -> bool {
        *self.latest.borrow() != self.id
    }

    /// Resolves once superseded (or once nothing issues changes any more)
    pub async fn wait(&mut self) {
        let _ = self.latest.wait_for(|latest| *latest != self.id).await;
    }
}

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

/// The validated password of the world being entered, for its signed comms login; `None` on
/// any other realm.
#[derive(Resource, Default)]
pub struct WorldCommsSecret(pub Option<String>);

/// `GET /world/<name>/permissions`, the parts that decide entry.
#[derive(Deserialize, Default)]
struct WorldPermissions {
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    permissions: Permissions,
}

#[derive(Deserialize, Default)]
struct Permissions {
    access: Option<AccessPermission>,
}

#[derive(Deserialize, Default)]
struct AccessPermission {
    #[serde(rename = "type", default)]
    ty: String,
    #[serde(default)]
    wallets: Option<Vec<String>>,
    #[serde(default)]
    communities: Option<Vec<String>>,
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

/// Whether the local user may enter `world`, and with what password. `known_secret` is a password
/// that got them in before, tried before asking. `Err` is the reason they may not, in words for
/// the player.
pub async fn check_world_access(
    realm: &str,
    world: &str,
    wallet: &Wallet,
    known_secret: Option<String>,
    client: &reqwest::Client,
    prompter: Option<&WorldPasswordPrompter>,
    superseded: &mut Superseded,
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
    // the name's owner is let in whatever the rule, as the worlds server does
    if permissions
        .owner
        .as_deref()
        .is_some_and(|owner| owner.eq_ignore_ascii_case(&address))
    {
        return Ok(None);
    }
    let access = permissions.permissions.access.unwrap_or_default();

    match access.ty.to_ascii_lowercase().as_str() {
        "" | "unrestricted" => Ok(None),
        "allow-list" => {
            if access
                .wallets
                .iter()
                .flatten()
                .any(|wallet| wallet.eq_ignore_ascii_case(&address))
            {
                return Ok(None);
            }
            for community in access.communities.iter().flatten() {
                if is_community_member(community, wallet, client).await {
                    return Ok(None);
                }
            }
            Err("you are not on this world's access list".to_owned())
        }
        "shared-secret" => {
            if let Some(password) = known_secret {
                if validate_password(realm, world, &password, wallet, client)
                    .await
                    .is_ok()
                {
                    return Ok(Some(password));
                }
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
                    Ok(()) => tokio::select! {
                        answer = answered => answer.ok().flatten(),
                        // drops the prompt unanswered, which withdraws it from the HUD
                        _ = superseded.wait() => return Err(SUPERSEDED.to_owned()),
                    },
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
    use super::{world_name, WorldPermissions};

    #[test]
    fn permissions_tolerate_sparse_records() {
        let record: WorldPermissions = serde_json::from_str(
            r#"{"owner":"0xab","permissions":{"access":{"type":"Allow-List","wallets":null}}}"#,
        )
        .unwrap();
        let access = record.permissions.access.unwrap();
        assert_eq!(access.ty, "Allow-List");
        assert!(access.wallets.is_none() && access.communities.is_none());

        let record: WorldPermissions =
            serde_json::from_str(r#"{"owner":null,"permissions":{"access":{}}}"#).unwrap();
        assert!(record.owner.is_none());
        assert_eq!(record.permissions.access.unwrap().ty, "");

        let record: WorldPermissions = serde_json::from_str(r#"{}"#).unwrap();
        assert!(record.permissions.access.is_none());
    }

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
        // the current realm is known by its about url
        assert_eq!(
            world_name("https://worlds-content-server.decentraland.org/world/boedo.dcl.eth/about"),
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
