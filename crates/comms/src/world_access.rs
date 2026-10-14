//! Entry check for worlds, answering `ipfs::world_access::RealmGate`. A world's `/permissions`
//! record names an access type — unrestricted, an allow-list of wallets and communities, or a
//! shared secret (a password). [`check_world_access`] runs the same check the unity explorer does
//! before the realm change is accepted, and for a password world validates the password with the
//! very comms handshake the worlds server enforces, so the change only goes ahead when the comms
//! login will.

use std::time::Duration;

use bevy::{prelude::*, tasks::IoTaskPool};
use common::{
    base_domain::{self, Service},
    util::TaskCompat,
};
use http::Uri;
use ipfs::{
    world_access::{
        RealmGate, RealmGateRequest, Superseded, WorldPasswordPrompt, WorldPasswordPrompter,
        SUPERSEDED,
    },
    IpfsAssetServer,
};
use serde::Deserialize;
use tokio::sync::{mpsc, oneshot};
use wallet::{sign_request, SignedLoginMeta, Wallet};

#[derive(Resource)]
pub struct RealmGateRequests(mpsc::UnboundedReceiver<RealmGateRequest>);

pub fn install_gate(app: &mut App) {
    let (sender, requests) = mpsc::unbounded_channel();
    app.insert_resource(RealmGate(sender));
    app.insert_resource(RealmGateRequests(requests));
}

pub fn answer_realm_gate(
    mut requests: ResMut<RealmGateRequests>,
    wallet: Option<Res<Wallet>>,
    prompter: Option<Res<WorldPasswordPrompter>>,
    ipfs: IpfsAssetServer,
) {
    while let Ok(request) = requests.0.try_recv() {
        // without an identity there is nothing to check against, and no comms to be kept out of
        let Some(wallet) = wallet.as_deref().filter(|w| w.address().is_some()).cloned() else {
            let _ = request.answer.send(Ok(None));
            continue;
        };
        let prompter = prompter.as_deref().cloned();
        let client = ipfs.ipfs().client();
        IoTaskPool::get()
            .spawn_compat(async move {
                let RealmGateRequest {
                    realm,
                    world,
                    known_secret,
                    mut superseded,
                    answer,
                } = request;
                let result = check_world_access(
                    &realm,
                    &world,
                    &wallet,
                    known_secret,
                    &client,
                    prompter.as_ref(),
                    &mut superseded,
                )
                .await;
                let _ = answer.send(result);
            })
            .detach();
    }
}

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
    let is_owner = address.eq_ignore_ascii_case(&permissions.owner);
    let access = permissions.permissions.access.unwrap_or_default();

    match access.ty.to_ascii_lowercase().as_str() {
        "" | "unrestricted" => Ok(None),
        "allow-list" => {
            if is_owner
                || access
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
            if is_owner {
                return Ok(None);
            }
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
    use super::WorldPermissions;

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
            serde_json::from_str(r#"{"permissions":{"access":{}}}"#).unwrap();
        assert_eq!(record.permissions.access.unwrap().ty, "");

        let record: WorldPermissions = serde_json::from_str(r#"{}"#).unwrap();
        assert!(record.permissions.access.is_none());
    }
}
