use std::{cell::RefCell, rc::Rc};

use anyhow::anyhow;
use bevy::log::debug;
use common::{
    rpc::{RpcCall, RpcResultSender},
    structs::SceneMeta,
    util::UrlLoopbackExt,
};
use serde::Serialize;
use url::Url;

use crate::{
    interface::crdt_context::CrdtContext,
    js::{player_identity, runtime::realm_information, State},
    RpcCalls,
};

#[derive(Serialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SignedFetchMetaRealm {
    hostname: String,
    protocol: String,
    server_name: String,
}

#[derive(Serialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SignedFetchMeta {
    origin: Option<String>,
    scene_id: Option<String>,
    parcel: Option<String>,
    tld: Option<String>,
    network: Option<String>,
    is_guest: Option<bool>,
    realm: SignedFetchMetaRealm,
    signer: String,
}

// the server's fake player has no identity crdt (#1102), and the server never signs as a guest
fn signed_fetch_is_guest(state: &impl State) -> Result<bool, anyhow::Error> {
    match player_identity(state) {
        Ok(identity) => Ok(identity.is_guest),
        Err(_) if state.borrow::<CrdtContext>().is_server => Ok(false),
        Err(e) => Err(e),
    }
}

pub async fn op_signed_fetch_headers(
    state: Rc<RefCell<impl State>>,
    uri: String,
    method: Option<String>,
) -> Result<Vec<(String, String)>, anyhow::Error> {
    debug!("op_signed_fetch_headers");

    let is_preview = state.borrow().borrow::<CrdtContext>().preview;
    let url = Url::parse(&uri)?;
    if !is_preview && !(["https", "wss"].contains(&url.scheme())) && !url.is_loopback() {
        anyhow::bail!("URL scheme must be `https` (request `{}`)", uri);
    }

    let realm_info = realm_information(state.clone()).await?;

    let is_guest = signed_fetch_is_guest(&*state.borrow())?;

    let urn = state.borrow().borrow::<CrdtContext>().hash.clone();

    let (sx, rx) = RpcResultSender::channel();
    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::EntityDefinition {
            urn: urn.clone(),
            response: sx,
        })?;

    let entity_definition = rx.await?.ok_or_else(|| anyhow!("no entity definition"))?;

    let scene_meta =
        serde_json::from_str::<SceneMeta>(&entity_definition.metadata.unwrap_or_default())?;

    let meta = SignedFetchMeta {
        origin: Some(realm_info.base_url.clone()),
        scene_id: Some(urn.clone()),
        parcel: Some(scene_meta.scene.base.clone()),
        tld: Some("org".to_owned()),
        network: Some("mainnet".to_owned()),
        is_guest: Some(is_guest),
        realm: SignedFetchMetaRealm {
            hostname: realm_info.base_url,
            protocol: "v3".to_owned(),
            server_name: realm_info.realm_name,
        },
        signer: "decentraland-kernel-scene".to_owned(),
    };

    debug!("signed fetch meta {:?}", meta);

    let (sx, rx) = RpcResultSender::channel();

    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::SignRequest {
            method: method.unwrap_or_else(|| String::from("get")),
            uri,
            meta: Some(serde_json::to_string(&meta).unwrap()),
            scene: Some(urn),
            response: sx,
        })?;

    rx.await?.map_err(|e| anyhow!(e))
}

#[cfg(test)]
mod signed_fetch_identity_tests {
    use super::*;
    use crate::{
        interface::{CrdtStore, CrdtType},
        js::{test_state::TestState, RendererStore},
        SceneId,
    };
    use dcl_component::{
        proto_components::sdk::components::PbPlayerIdentityData, DclReader, DclWriter,
        SceneComponentId, SceneEntityId, ToDclWriter,
    };

    fn state(is_server: bool, identity: Option<PbPlayerIdentityData>) -> TestState {
        let mut store = CrdtStore::default();
        if let Some(identity) = identity {
            let mut buf = Vec::new();
            identity.to_writer(&mut DclWriter::new(&mut buf));
            store.force_update(
                SceneComponentId::PLAYER_IDENTITY_DATA,
                CrdtType::LWW_ANY,
                SceneEntityId::PLAYER,
                Some(&mut DclReader::new(&buf)),
            );
        }
        let mut s = TestState::default();
        s.put(RendererStore(store));
        s.put(CrdtContext::new(
            SceneId::DUMMY,
            Default::default(),
            Default::default(),
            false,
            false,
            is_server,
        ));
        s
    }

    #[test]
    fn server_without_identity_is_not_guest() {
        assert!(!signed_fetch_is_guest(&state(true, None)).unwrap());
    }

    #[test]
    fn client_without_identity_still_errors() {
        assert!(signed_fetch_is_guest(&state(false, None)).is_err());
    }

    #[test]
    fn present_identity_wins_over_fallback() {
        let identity = PbPlayerIdentityData {
            address: "0x1234".to_owned(),
            is_guest: true,
        };
        assert!(signed_fetch_is_guest(&state(true, Some(identity))).unwrap());
    }
}
