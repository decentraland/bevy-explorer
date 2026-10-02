// World-storage delegation: an orchestrator-minted, short-lived credential that lets a
// headless authoritative server sign world-storage requests without ever holding the
// authoritative key. Wire envelope (base64 JSON, mirrors hammurabi-headless):
// { "v": 1, "ephemeral": { "privateKey", "publicKey", "address" },
//   "scope": { "payload", "signature" } }
// The scene-scope fields (World/SceneId/Parcel/Expiration) are derived from the signed
// scope.payload lines — the single source of truth — never from unsigned copies.

use std::{collections::HashMap, str::FromStr};

use alloy_core::primitives::Address;
use alloy_signer_local::PrivateKeySigner;
use base64::Engine;
use bevy::prelude::*;
use serde::Deserialize;

use crate::Wallet;

pub const STORAGE_HOSTS: [&str; 2] = ["storage.decentraland.org", "storage.decentraland.zone"];
pub const BADGES_HOSTS: [&str; 2] = ["badges.decentraland.org", "badges.decentraland.zone"];

#[derive(Deserialize)]
struct DelegationEnvelope {
    v: u32,
    ephemeral: DelegationEphemeral,
    scope: DelegationScope,
}

#[derive(Deserialize)]
struct DelegationEphemeral {
    #[serde(rename = "privateKey")]
    private_key: String,
    address: String,
}

#[derive(Deserialize, serde::Serialize, Clone)]
pub struct DelegationScope {
    pub payload: String,
    pub signature: String,
}

#[derive(Clone)]
pub struct StorageDelegation {
    /// standalone ephemeral signer (owner = ephemeral address; authorized only by the scope claim)
    pub wallet: Wallet,
    /// pre-encoded `x-authoritative-scope` header value: base64(json(scope))
    pub scope_header: String,
    /// replacement `x-identity-metadata` JSON, built wholesale from the delegation claims
    pub meta: String,
    pub world: String,
    pub scene_id: String,
    pub parcel: String,
    /// unix millis
    pub expiration: i64,
}

impl StorageDelegation {
    /// Decode and validate a base64 delegation. `realm_hostname` is only reported in the
    /// request metadata (the verifier keys on the claim's world/sceneId/parcel).
    /// Errors never echo decoded content — it contains the ephemeral private key.
    pub fn parse(encoded: &str, realm_hostname: &str) -> Result<Self, anyhow::Error> {
        let json = base64::engine::general_purpose::STANDARD
            .decode(encoded.trim())
            .map_err(|_| anyhow::anyhow!("storage delegation is not valid base64"))?;
        let envelope: DelegationEnvelope = serde_json::from_slice(&json)
            .map_err(|_| anyhow::anyhow!("storage delegation envelope is malformed"))?;
        if envelope.v != 1 {
            anyhow::bail!("unsupported storage delegation version {}", envelope.v);
        }

        let value_for = |prefix: &str| -> Option<String> {
            envelope
                .scope
                .payload
                .lines()
                .find(|l| l.starts_with(prefix))
                .map(|l| l[prefix.len()..].trim().to_owned())
        };
        let world = value_for("World:")
            .map(|w| w.to_lowercase())
            .ok_or_else(|| anyhow::anyhow!("delegation claim missing World"))?;
        let scene_id = value_for("SceneId:")
            .ok_or_else(|| anyhow::anyhow!("delegation claim missing SceneId"))?;
        let parcel = value_for("Parcel:")
            .ok_or_else(|| anyhow::anyhow!("delegation claim missing Parcel"))?;
        let expiration_iso = value_for("Expiration:")
            .ok_or_else(|| anyhow::anyhow!("delegation claim missing Expiration"))?;
        let expiration = chrono::DateTime::parse_from_rfc3339(&expiration_iso)
            .map_err(|_| anyhow::anyhow!("delegation claim has unparseable Expiration"))?
            .timestamp_millis();

        let local_wallet =
            PrivateKeySigner::from_str(envelope.ephemeral.private_key.trim_start_matches("0x"))
                .map_err(|_| anyhow::anyhow!("delegation ephemeral key is invalid"))?;
        let claimed = envelope.ephemeral.address.to_lowercase();
        let derived = format!("{:#x}", local_wallet.address());
        if claimed != derived {
            anyhow::bail!("delegation ephemeral address does not match its key");
        }
        let mut wallet = Wallet::default();
        wallet.finalize(local_wallet.address(), local_wallet, Vec::default());

        let scope_header = base64::engine::general_purpose::STANDARD
            .encode(serde_json::to_string(&envelope.scope)?);

        // deliberately the DELEGATION's world/sceneId/parcel — the storage service derives
        // the placeId the scope claim is bound to from these (hammurabi parity, including
        // the historical 'hammurabi-server//' origin embedded in signed payloads)
        let meta = serde_json::json!({
            "origin": "hammurabi-server//",
            "signer": "dcl:authoritative-server",
            "isGuest": false,
            "realm": { "serverName": world, "hostname": realm_hostname },
            "realmName": world,
            "sceneId": scene_id,
            "parcel": parcel,
        })
        .to_string();

        Ok(Self {
            wallet,
            scope_header,
            meta,
            world,
            scene_id,
            parcel,
            expiration,
        })
    }

    pub fn is_expired(&self, now_millis: i64) -> bool {
        now_millis >= self.expiration
    }

    /// The signed metadata for a badge award: [`Self::meta`] plus `targetAddress`, the player
    /// the award names. The badges service compares it to the award path's player, so the
    /// worker only writes it after confirming that player is present in the scene's room —
    /// scene JS chooses the URL, the engine decides whether the name gets signed.
    pub fn meta_with_target(&self, target: Address) -> String {
        let mut meta: serde_json::Value =
            serde_json::from_str(&self.meta).expect("meta is built by parse() as a json object");
        meta["targetAddress"] = serde_json::Value::String(format!("{target:#x}"));
        meta.to_string()
    }
}

/// Per-scene world-storage delegations, plus a fallback slot for single-scene
/// (non-orchestrated) runs where the scene hash isn't known at startup.
#[derive(Resource, Default)]
pub struct StorageDelegations {
    pub by_scene: HashMap<String, StorageDelegation>,
    pub fallback: Option<StorageDelegation>,
}

impl StorageDelegations {
    pub fn get(&self, scene: &str) -> Option<&StorageDelegation> {
        // the fallback is still bound to the scene its claim was minted for (claim SceneId
        // and the lookup key are both the scene entity hash) — it must never become a
        // wildcard credential for co-tenant scenes
        self.by_scene
            .get(scene)
            .or_else(|| self.fallback.as_ref().filter(|d| d.scene_id == scene))
    }
}

/// True when a signed fetch to `uri` must be signed with a storage delegation: exact-match
/// world-storage hosts, or the resolved storage service (host and port; an explicit override
/// chooses them). https only either way — the claim must never go out in cleartext, so an http
/// storage override is simply never signed for.
pub fn is_storage_request(uri: &http::Uri) -> bool {
    fn https_origin(u: &http::Uri) -> Option<(String, u16)> {
        if u.scheme_str() != Some("https") {
            return None;
        }
        Some((u.host()?.to_lowercase(), u.port_u16().unwrap_or(443)))
    }
    let Some((host, port)) = https_origin(uri) else {
        return false;
    };
    STORAGE_HOSTS.contains(&host.as_str())
        || common::base_domain::service(common::base_domain::Service::Storage)
            .parse::<http::Uri>()
            .ok()
            .and_then(|storage| https_origin(&storage))
            == Some((host, port))
}

/// True when a signed fetch to `uri` targets the badges service and may be signed with the
/// scene's delegation: exact-match badges hosts over https, or the resolved badges service
/// (host and port). Unlike storage, a *loopback* http override is accepted, so a local award
/// stub can be driven from a preview run; any other http origin is never signed for.
pub fn is_badges_request(uri: &http::Uri) -> bool {
    fn origin(u: &http::Uri) -> Option<(String, String, u16)> {
        let scheme = u.scheme_str()?.to_owned();
        let host = u.host()?.to_lowercase();
        let port = u.port_u16().unwrap_or(if scheme == "https" { 443 } else { 80 });
        Some((scheme, host, port))
    }
    let Some((scheme, host, port)) = origin(uri) else {
        return false;
    };
    if scheme == "https" && BADGES_HOSTS.contains(&host.as_str()) {
        return true;
    }
    let Some((o_scheme, o_host, o_port)) =
        common::base_domain::service(common::base_domain::Service::Badges)
            .parse::<http::Uri>()
            .ok()
            .and_then(|badges| origin(&badges))
    else {
        return false;
    };
    if (o_scheme, o_host.as_str(), o_port) != (scheme.clone(), host.as_str(), port) {
        return false;
    }
    scheme == "https" || is_loopback(&host)
}

fn is_loopback(host: &str) -> bool {
    host == "localhost"
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

/// The player a badge award names, from the award URL the SDK builds:
/// `/worlds/{world}/badges/{badgeId}/awards/{player}`. `None` for any other path, so a
/// delegation is never attached to a request shape the badges service doesn't expect.
pub fn award_target(uri: &http::Uri) -> Option<Address> {
    let mut segments = uri.path().trim_matches('/').split('/');
    let shape = [
        segments.next()?,
        segments.next()?,
        segments.next()?,
        segments.next()?,
        segments.next()?,
    ];
    let player = segments.next()?;
    if segments.next().is_some() {
        return None;
    }
    if shape[0] != "worlds" || shape[2] != "badges" || shape[4] != "awards" {
        return None;
    }
    if shape[1].is_empty() || shape[3].is_empty() {
        return None;
    }
    player.parse::<Address>().ok()
}

#[cfg(test)]
mod test {
    use super::*;

    fn mint(expiration: &str) -> String {
        // throwaway key
        let payload = format!(
            "Decentraland Authoritative Storage Delegation\nEphemeral: 0x63f9a92d8d61b48a9fff8d58080425a3012d05c8\nWorld: Boedo.dcl.eth\nSceneId: bafkreitest\nParcel: 0,0\nExpiration: {expiration}"
        );
        let envelope = serde_json::json!({
            "v": 1,
            "ephemeral": {
                "privateKey": "0000000000000000000000000000000000000000000000000000000000000001",
                "publicKey": "unused",
                "address": "0x7e5f4552091a69125d5dfcb7b8c2659029395bdf",
            },
            "scope": { "payload": payload, "signature": "0xsig" },
        });
        base64::engine::general_purpose::STANDARD.encode(envelope.to_string())
    }

    #[test]
    fn parses_and_lowercases_world() {
        let d = StorageDelegation::parse(&mint("2030-01-01T00:00:00Z"), "https://realm").unwrap();
        assert_eq!(d.world, "boedo.dcl.eth");
        assert_eq!(d.scene_id, "bafkreitest");
        assert_eq!(d.parcel, "0,0");
        assert!(!d.is_expired(chrono::Utc::now().timestamp_millis()));
        assert!(d.meta.contains("\"signer\":\"dcl:authoritative-server\""));
    }

    #[test]
    fn rejects_mismatched_ephemeral_address() {
        let payload = "Decentraland Authoritative Storage Delegation\nWorld: w\nSceneId: s\nParcel: 0,0\nExpiration: 2030-01-01T00:00:00Z";
        let envelope = serde_json::json!({
            "v": 1,
            "ephemeral": {
                "privateKey": "0000000000000000000000000000000000000000000000000000000000000001",
                "publicKey": "unused",
                "address": "0x0000000000000000000000000000000000000dead",
            },
            "scope": { "payload": payload, "signature": "0xsig" },
        });
        let encoded = base64::engine::general_purpose::STANDARD.encode(envelope.to_string());
        assert!(StorageDelegation::parse(&encoded, "").is_err());
    }

    #[test]
    fn fallback_only_serves_its_own_scene() {
        let delegations = StorageDelegations {
            fallback: Some(
                StorageDelegation::parse(&mint("2030-01-01T00:00:00Z"), "https://realm").unwrap(),
            ),
            ..Default::default()
        };
        assert!(delegations.get("bafkreitest").is_some());
        assert!(delegations.get("bafkreiother").is_none());
    }

    #[test]
    fn storage_host_matching() {
        assert!(is_storage_request(
            &"https://storage.decentraland.org/values/x".parse().unwrap()
        ));
        assert!(!is_storage_request(
            &"http://storage.decentraland.org/values/x".parse().unwrap()
        ));
        assert!(!is_storage_request(
            &"https://storage.decentraland.org.evil.com/values"
                .parse()
                .unwrap()
        ));
    }

    #[test]
    fn badges_host_matching() {
        assert!(is_badges_request(
            &"https://badges.decentraland.org/worlds/w/badges/b/awards/0x1"
                .parse()
                .unwrap()
        ));
        assert!(is_badges_request(
            &"https://badges.decentraland.zone/x".parse().unwrap()
        ));
        assert!(!is_badges_request(
            &"http://badges.decentraland.org/x".parse().unwrap()
        ));
        assert!(!is_badges_request(
            &"https://badges.decentraland.org.evil.com/x".parse().unwrap()
        ));
        // storage is not badges and vice versa
        assert!(!is_badges_request(
            &"https://storage.decentraland.org/values/x".parse().unwrap()
        ));
        assert!(!is_storage_request(
            &"https://badges.decentraland.org/x".parse().unwrap()
        ));
    }

    #[test]
    fn award_target_parses_only_the_award_shape() {
        let player: Address = "0x63f9a92d8d61b48a9fff8d58080425a3012d05c8"
            .parse()
            .unwrap();
        assert_eq!(
            award_target(
                &"https://badges.decentraland.org/worlds/boedo.dcl.eth/badges/marathon/awards/0x63f9a92d8d61b48a9fff8d58080425a3012d05c8"
                    .parse()
                    .unwrap()
            ),
            Some(player)
        );
        // trailing slash and mixed-case address are fine
        assert_eq!(
            award_target(
                &"https://badges.decentraland.org/worlds/w/badges/b/awards/0x63F9A92D8D61B48A9FFF8D58080425A3012D05C8/"
                    .parse()
                    .unwrap()
            ),
            Some(player)
        );
        for bad in [
            "https://badges.decentraland.org/worlds/w/badges/b",
            "https://badges.decentraland.org/worlds/w/badges/b/awards/notanaddress",
            "https://badges.decentraland.org/worlds/w/badges/b/awards/0x63f9a92d8d61b48a9fff8d58080425a3012d05c8/extra",
            "https://badges.decentraland.org/scenes/s/badges/b/awards/0x63f9a92d8d61b48a9fff8d58080425a3012d05c8",
            "https://badges.decentraland.org/worlds//badges/b/awards/0x63f9a92d8d61b48a9fff8d58080425a3012d05c8",
            "https://badges.decentraland.org/users/0x63f9a92d8d61b48a9fff8d58080425a3012d05c8/badges",
        ] {
            assert_eq!(award_target(&bad.parse().unwrap()), None, "{bad}");
        }
    }

    #[test]
    fn meta_with_target_adds_the_player_and_keeps_the_scope_fields() {
        let d = StorageDelegation::parse(&mint("2030-01-01T00:00:00Z"), "https://realm").unwrap();
        let player: Address = "0x63F9A92D8D61B48A9FFF8D58080425A3012D05C8"
            .parse()
            .unwrap();
        let meta: serde_json::Value = serde_json::from_str(&d.meta_with_target(player)).unwrap();
        assert_eq!(
            meta["targetAddress"],
            "0x63f9a92d8d61b48a9fff8d58080425a3012d05c8"
        );
        assert_eq!(meta["signer"], "dcl:authoritative-server");
        assert_eq!(meta["realmName"], "boedo.dcl.eth");
        assert_eq!(meta["sceneId"], "bafkreitest");
        // the plain meta never carries a target
        assert!(!d.meta.contains("targetAddress"));
    }

    // NOTE: latches the process-wide service overrides — the only test in this crate that may.
    #[test]
    fn storage_override_matching() {
        common::base_domain::set_services([
            (
                common::base_domain::Service::Storage,
                "https://storage.example:8443/",
            ),
            (
                common::base_domain::Service::Badges,
                "http://localhost:4000/",
            ),
        ])
        .unwrap();
        // a loopback http badges override is signed for (local award stub); the known hosts
        // stay signed for alongside it; a non-loopback http origin never is
        assert!(is_badges_request(
            &"http://localhost:4000/worlds/w/badges/b/awards/0x1".parse().unwrap()
        ));
        assert!(!is_badges_request(
            &"http://localhost:4001/worlds/w/badges/b/awards/0x1".parse().unwrap()
        ));
        assert!(is_badges_request(
            &"https://badges.decentraland.org/x".parse().unwrap()
        ));
        assert!(!is_badges_request(
            &"http://badges.example:4000/x".parse().unwrap()
        ));
        assert!(is_storage_request(
            &"https://storage.example:8443/values/x".parse().unwrap()
        ));
        // the known hosts stay signed for alongside an override
        assert!(is_storage_request(
            &"https://storage.decentraland.org/values/x".parse().unwrap()
        ));
        // port is part of the origin (default 443 when implicit), and https is not negotiable
        assert!(!is_storage_request(
            &"https://storage.example/values/x".parse().unwrap()
        ));
        assert!(!is_storage_request(
            &"http://storage.example:8443/values/x".parse().unwrap()
        ));
    }
}
