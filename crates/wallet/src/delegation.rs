// World-storage delegation: an orchestrator-minted, short-lived credential that lets a
// headless authoritative server sign world-storage requests without ever holding the
// authoritative key. Wire envelope (base64 JSON, mirrors hammurabi-headless):
// { "v": 1, "ephemeral": { "privateKey", "publicKey", "address" },
//   "scope": { "payload", "signature" } }
// The scene-scope fields (World/SceneId/Parcel/Expiration) are derived from the signed
// scope.payload lines — the single source of truth — never from unsigned copies.

use std::{collections::HashMap, str::FromStr};

use base64::Engine;
use bevy::prelude::*;
use ethers_signers::{LocalWallet, Signer};
use serde::Deserialize;

use crate::Wallet;

pub const STORAGE_HOSTS: [&str; 2] = ["storage.decentraland.org", "storage.decentraland.zone"];

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
        let scene_id =
            value_for("SceneId:").ok_or_else(|| anyhow::anyhow!("delegation claim missing SceneId"))?;
        let parcel =
            value_for("Parcel:").ok_or_else(|| anyhow::anyhow!("delegation claim missing Parcel"))?;
        let expiration_iso = value_for("Expiration:")
            .ok_or_else(|| anyhow::anyhow!("delegation claim missing Expiration"))?;
        let expiration = chrono::DateTime::parse_from_rfc3339(&expiration_iso)
            .map_err(|_| anyhow::anyhow!("delegation claim has unparseable Expiration"))?
            .timestamp_millis();

        let local_wallet =
            LocalWallet::from_str(envelope.ephemeral.private_key.trim_start_matches("0x"))
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

/// True when a signed fetch to `uri` must be signed with a storage delegation:
/// exact-match world-storage hosts, https only (the claim must never go out in cleartext).
pub fn is_storage_request(uri: &http::Uri) -> bool {
    uri.scheme_str() == Some("https")
        && uri
            .host()
            .is_some_and(|h| STORAGE_HOSTS.contains(&h.to_lowercase().as_str()))
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
        let mut delegations = StorageDelegations::default();
        delegations.fallback =
            Some(StorageDelegation::parse(&mint("2030-01-01T00:00:00Z"), "https://realm").unwrap());
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
            &"https://storage.decentraland.org.evil.com/values".parse().unwrap()
        ));
    }
}
