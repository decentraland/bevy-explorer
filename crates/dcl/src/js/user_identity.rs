use std::{cell::RefCell, rc::Rc};

use bevy::log::debug;
use common::{profile::SerializedProfile, rpc::{RpcCall, RpcResultSender}};
use serde::Serialize;

use crate::{interface::crdt_context::CrdtContext, RpcCalls};

use super::State;

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Snapshots {
    face256: String,
    body: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AvatarForUserData {
    body_shape: String,
    skin_color: String,
    hair_color: String,
    eye_color: String,
    wearables: Vec<String>,
    snapshots: Option<Snapshots>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UserData {
    display_name: String,
    pub public_key: Option<String>,
    has_connected_web3: bool,
    user_id: String,
    version: i64,
    avatar: Option<AvatarForUserData>,
    has_claimed_name: bool,
}

pub struct UserEthAddress(pub String);

pub async fn op_get_user_data(state: Rc<RefCell<impl State>>) -> Result<UserData, anyhow::Error> {
    let (sx, rx) = RpcResultSender::<Result<SerializedProfile, ()>>::channel();

    let scene = state.borrow().borrow::<CrdtContext>().scene_id.0;
    debug!("[{scene:?}] -> op_get_user_data");

    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::GetUserData {
            user: None, // current user
            scene,
            response: sx,
        });

    let profile = rx
        .await
        .map_err(|e| anyhow::anyhow!(e))?
        .map_err(|_| anyhow::anyhow!("Not found"))?;

    state
        .borrow_mut()
        .put(UserEthAddress(profile.eth_address.clone()));

    let user_data = profile.into();

    debug!("[{scene:?}] <- get_user_data {user_data:?}");
    Ok(user_data)
}

pub async fn op_get_player_data(
    state: Rc<RefCell<impl State>>,
    id: String,
) -> Result<UserData, anyhow::Error> {
    let (sx, rx) = RpcResultSender::<Result<SerializedProfile, ()>>::channel();

    let scene = state.borrow().borrow::<CrdtContext>().scene_id.0;
    debug!("[{scene:?}] -> op_get_player_data");

    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::GetUserData {
            user: Some(id),
            scene,
            response: sx,
        });

    rx.await
        .map_err(|e| anyhow::anyhow!(e))?
        .map(Into::into)
        .map_err(|_| anyhow::anyhow!("Not found"))
}

impl From<SerializedProfile> for UserData {
    fn from(profile: SerializedProfile) -> Self {
        Self {
            display_name: profile.name,
            public_key: Some(profile.eth_address.clone()),
            has_connected_web3: profile.has_connected_web3.unwrap_or_default(),
            user_id: profile.user_id.unwrap_or_default(),
            version: profile.version,
            avatar: Some(AvatarForUserData {
                body_shape: profile.avatar.body_shape.unwrap_or_default(),
                skin_color: serde_json::to_string(&profile.avatar.skin.unwrap_or_default())
                    .unwrap(),
                hair_color: serde_json::to_string(&profile.avatar.hair.unwrap_or_default())
                    .unwrap(),
                eye_color: serde_json::to_string(&profile.avatar.eyes.unwrap_or_default()).unwrap(),
                wearables: profile.avatar.wearables,
                snapshots: profile.avatar.snapshots.map(|s| Snapshots {
                    face256: s.face256,
                    body: s.body,
                }),
            }),
            has_claimed_name: profile.has_claimed_name,
        }
    }
}
