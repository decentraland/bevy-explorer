use std::{io::Read, path::PathBuf, sync::Arc};

use anyhow::anyhow;
use bevy::{
    ecs::system::SystemParam,
    prelude::*,
    tasks::{IoTaskPool, Task},
    utils::HashMap,
};
use dcl::interface::CrdtType;
use ethers_core::types::Address;
use ipfs::{ipfs_path::IpfsPath, IpfsAssetServer, IpfsIo, TypedIpfsRef};
use multihash_codetable::MultihashDigest;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

use crate::global_crdt::GlobalCrdtState;

use super::{
    global_crdt::{process_transport_updates, ForeignPlayer, ProfileEvent, ProfileEventType},
    NetworkMessage, Transport,
};
use common::{
    profile::{AvatarSnapshots, LambdaProfiles, SerializedProfile},
    rpc::RpcEventSender,
    structs::PrimaryUser,
    util::TaskExt,
};
use common::{rpc::RpcCall, util::AsH160};
use dcl_component::{
    proto_components::{kernel::comms::rfc4, sdk::components::PbPlayerIdentityData},
    SceneComponentId, SceneEntityId,
};
use wallet::Wallet;

pub struct UserProfilePlugin;

impl Plugin for UserProfilePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                request_missing_profiles,
                process_profile_events,
                setup_primary_profile,
            )
                .before(process_transport_updates), // .in_set(TODO)
        );

        app.insert_resource(CurrentUserProfile::default());
        app.init_resource::<ProfileCache>()
            .init_resource::<ProfileMetaCache>();
    }
}

enum ProfileDisplayState {
    Loaded(Box<UserProfile>),
    Loading(Task<Result<UserProfile, anyhow::Error>>),
    Failed,
}

#[derive(Resource, Default)]
pub struct ProfileCache(HashMap<Address, ProfileDisplayState>);
#[derive(Resource, Default)]
pub struct ProfileMetaCache(pub HashMap<Address, String>);

#[derive(SystemParam)]
pub struct ProfileManager<'w, 's> {
    cache: ResMut<'w, ProfileCache>,
    meta_cache: ResMut<'w, ProfileMetaCache>,
    ipfs: IpfsAssetServer<'w, 's>,
}

pub struct ProfileMissingError;

impl ProfileManager<'_, '_> {
    pub fn get_data(
        &mut self,
        address: Address,
    ) -> Result<Option<&UserProfile>, ProfileMissingError> {
        let state = self.cache.0.entry(address).or_insert_with(|| {
            ProfileDisplayState::Loading(IoTaskPool::get().spawn(get_remote_profile(
                address,
                self.ipfs.ipfs().clone(),
                self.meta_cache.0.get(&address).cloned(),
            )))
        });

        if let ProfileDisplayState::Loading(task) = state {
            match task.complete() {
                Some(Ok(profile)) => *state = ProfileDisplayState::Loaded(Box::new(profile)),
                Some(Err(_)) => *state = ProfileDisplayState::Failed,
                None => (),
            }
        }

        Ok(match state {
            ProfileDisplayState::Loaded(data) => Some(data),
            ProfileDisplayState::Loading(_) => None,
            ProfileDisplayState::Failed => return Err(ProfileMissingError),
        })
    }

    pub fn get_image(
        &mut self,
        address: Address,
    ) -> Result<Option<Handle<Image>>, ProfileMissingError> {
        let profile = self.get_data(address)?;
        let Some(profile) = profile else {
            return Ok(None);
        };
        let Some(path) = profile
            .content
            .avatar
            .snapshots
            .as_ref()
            .and_then(|snapshots| {
                if snapshots.face256.is_empty() {
                    None
                } else {
                    let url = format!("{}{}", profile.base_url, snapshots.face256);
                    let ipfs_path = IpfsPath::new_from_url(&url, "png");
                    Some(PathBuf::from(&ipfs_path))
                }
            })
        else {
            return Err(ProfileMissingError);
        };
        Ok(Some(self.ipfs.asset_server().load(path)))
    }

    pub fn get_name(&mut self, address: Address) -> Result<Option<&String>, ProfileMissingError> {
        Ok(self.get_data(address)?.map(|profile| &profile.content.name))
    }

    pub fn update(&mut self, profile: UserProfile) {
        if let Some(address) = profile.content.eth_address.as_h160() {
            self.cache
                .0
                .insert(address, ProfileDisplayState::Loaded(Box::new(profile)));
        }
    }

    pub fn remove(&mut self, address: Address) {
        self.cache.0.remove(&address);
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn setup_primary_profile(
    mut commands: Commands,
    player: Query<(Entity, Option<&UserProfile>), With<PrimaryUser>>,
    mut current_profile: ResMut<CurrentUserProfile>,
    transports: Query<&Transport>,
    mut senders: Local<Vec<RpcEventSender>>,
    mut subscribe_events: EventReader<RpcCall>,
    mut deploy_task: Local<Option<Task<Result<Option<(String, String)>, anyhow::Error>>>>,
    wallet: Res<Wallet>,
    ipfas: IpfsAssetServer,
    images: Res<Assets<Image>>,
    mut global_crdt: ResMut<GlobalCrdtState>,
    mut cache: ProfileManager,
    mut last_announce: Local<f32>,
    time: Res<Time>,
) {
    // gather any event receivers
    for sender in subscribe_events.read().filter_map(|ev| match ev {
        RpcCall::SubscribeProfileChanged { sender } => Some(sender),
        _ => None,
    }) {
        senders.push(sender.clone());
    }

    if let Ok((player, maybe_profile)) = player.get_single() {
        if maybe_profile.is_none() || current_profile.is_changed() {
            let Some(profile) = current_profile.profile.as_ref() else {
                commands.entity(player).remove::<UserProfile>();
                return;
            };

            // update component
            commands.entity(player).try_insert(profile.clone());

            // update cache
            cache.update(profile.clone());

            // send to scenes
            global_crdt.update_crdt(
                SceneComponentId::PLAYER_IDENTITY_DATA,
                CrdtType::LWW_ANY,
                SceneEntityId::PLAYER,
                &PbPlayerIdentityData {
                    address: profile.content.eth_address.clone(),
                    is_guest: !(profile.content.has_connected_web3.unwrap_or(false)),
                },
            );

            // send over network
            debug!("sending profile new version {:?}", profile.version);
            let response = rfc4::Packet {
                message: Some(rfc4::packet::Message::ProfileResponse(
                    rfc4::ProfileResponse {
                        serialized_profile: serde_json::to_string(&profile.content).unwrap(),
                        base_url: profile.base_url.clone(),
                    },
                )),
                protocol_version: 100,
            };
            for transport in &transports {
                let _ = transport
                    .sender
                    .try_send(NetworkMessage::reliable(&response));
            }

            // send to event receivers
            senders.retain(|sender| {
                let _ = sender.send(format!(
                    "{{ \"ethAddress\": \"{}\", \"version\": \"{}\" }}",
                    profile.content.user_id.as_ref().unwrap(),
                    profile.version
                ));
                !sender.is_closed()
            });

            // deploy to server
            if !current_profile.is_deployed {
                debug!("deploying {:#?}", profile);
                let ipfs = ipfas.ipfs().clone();
                let profile = profile.clone();
                let wallet = wallet.clone();
                *deploy_task = Some(IoTaskPool::get().spawn(deploy_profile(
                    ipfs,
                    wallet,
                    profile,
                    current_profile.snapshots.as_ref().and_then(|sn| {
                        if let (Some(face), Some(body)) = (
                            images.get(sn.0.id()).cloned(),
                            images.get(sn.1.id()).cloned(),
                        ) {
                            Some((face, body))
                        } else {
                            None
                        }
                    }),
                )));
                current_profile.is_deployed = true;
            }
        } else if let Some(current_profile) = current_profile.profile.as_ref() {
            let now = time.elapsed_seconds();
            if now > *last_announce + 5.0 {
                debug!("announcing profile v {}", current_profile.version);
                let packet = rfc4::Packet {
                    message: Some(rfc4::packet::Message::ProfileVersion(
                        rfc4::AnnounceProfileVersion {
                            profile_version: current_profile.version,
                        },
                    )),
                    protocol_version: 100,
                };
                for transport in transports.iter() {
                    let _ = transport.sender.try_send(NetworkMessage::reliable(&packet));
                }
                *last_announce = now;
            }
        }
    }

    if let Some(mut task) = deploy_task.take() {
        match task.complete() {
            Some(Ok(None)) => {
                info!("deployed profile ok");
            }
            Some(Ok(Some((face256, body)))) => {
                info!("deployed profile ok (with snapshots)");
                current_profile
                    .profile
                    .as_mut()
                    .unwrap()
                    .content
                    .avatar
                    .snapshots = Some(AvatarSnapshots { face256, body });
                current_profile.snapshots = None;
                cache.update(current_profile.profile.clone().unwrap());
            }
            Some(Err(e)) => {
                error!("failed to deploy profile: {e}");
                // todo toast
            }
            None => *deploy_task = Some(task),
        }
    }
}

#[derive(Resource, Default)]
pub struct CurrentUserProfile {
    pub profile: Option<UserProfile>,
    pub snapshots: Option<(Handle<Image>, Handle<Image>)>,
    pub is_deployed: bool,
}

#[allow(clippy::too_many_arguments)]
fn request_missing_profiles(
    mut commands: Commands,
    profiles: Query<(Entity, &mut ForeignPlayer, Option<&UserProfile>)>,
    mut manager: ProfileManager,
    mut global_crdt: ResMut<GlobalCrdtState>,
    mut requested: Local<HashMap<Address, f32>>,
    transports: Query<&Transport>,
    time: Res<Time>,
) {
    let mut last_requested = std::mem::take(&mut *requested);

    for (ent, player, _) in profiles.iter().filter(|(_, player, maybe_profile)| {
        maybe_profile.is_none_or(|profile| player.profile_version > profile.version)
    }) {
        if let Some((address, req_time)) = last_requested.remove_entry(&player.address) {
            if time.elapsed_seconds() - req_time < 10.0 {
                requested.insert(address, req_time);
                continue;
            }
        }

        let dbb = manager.meta_cache.0.get(&player.address).cloned();
        match manager.get_data(player.address) {
            Ok(Some(profile)) => {
                // catalyst fetch complete
                if profile.version >= player.profile_version {
                    global_crdt.update_crdt(
                        SceneComponentId::PLAYER_IDENTITY_DATA,
                        CrdtType::LWW_ANY,
                        player.scene_id,
                        &PbPlayerIdentityData {
                            address: format!("{:#x}", player.address),
                            is_guest: !(profile.content.has_connected_web3.unwrap_or(false)),
                        },
                    );

                    commands.entity(ent).try_insert(profile.clone());
                } else {
                    warn!(
                        "removing stale profile {} != {} (meta = {:?})",
                        profile.version, player.profile_version, dbb,
                    );
                    manager.remove(player.address);
                }
                continue;
            }
            Ok(None) => {
                // catalyst fetch in progress
                continue;
            }
            Err(_) => {
                // catalyst doesn't have the data, fallback to comms profile request
            }
        }

        if let Ok(transport) = transports.get(player.transport_id) {
            let request = rfc4::Packet {
                message: Some(rfc4::packet::Message::ProfileRequest(
                    rfc4::ProfileRequest {
                        address: format!("{:#x}", player.address),
                        profile_version: player.profile_version,
                    },
                )),
                protocol_version: 100,
            };
            match transport
                .sender
                .try_send(NetworkMessage::unreliable(&request))
            {
                Err(e) => {
                    warn!("failed to send request: {e}");
                }
                Ok(_) => {
                    debug!("sent profile request for player {player:?}");
                }
            };
            requested.insert(player.address, time.elapsed_seconds());
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn process_profile_events(
    mut commands: Commands,
    mut players: Query<(&mut ForeignPlayer, Option<&mut UserProfile>)>,
    mut events: EventReader<ProfileEvent>,
    mut last_sent_request: Local<HashMap<Entity, f32>>,
    time: Res<Time>,
    wallet: Res<Wallet>,
    transports: Query<&Transport>,
    current_profile: Res<CurrentUserProfile>,
    mut global_crdt: ResMut<GlobalCrdtState>,
    mut cache: ProfileManager,
) {
    for ev in events.read() {
        match &ev.event {
            ProfileEventType::Request(r) => {
                if let Some(req_address) = r.address.as_h160() {
                    if Some(req_address) == wallet.address() {
                        let Ok((player, _)) = players.get(ev.sender) else {
                            continue;
                        };
                        if last_sent_request.get(&player.transport_id).is_some() {
                            debug!("ignoring request for my profile (sent recently)");
                            continue;
                        }

                        let Ok(transport) = transports.get(player.transport_id) else {
                            debug!("not sending profile, no transport");
                            continue;
                        };

                        let Some(current_profile) = current_profile.profile.as_ref() else {
                            return;
                        };

                        debug!("sending my profile");
                        let response = rfc4::Packet {
                            message: Some(rfc4::packet::Message::ProfileResponse(
                                rfc4::ProfileResponse {
                                    serialized_profile: serde_json::to_string(
                                        &current_profile.content,
                                    )
                                    .unwrap(),
                                    base_url: current_profile.base_url.clone(),
                                },
                            )),
                            protocol_version: 100,
                        };
                        let _ = transport
                            .sender
                            .try_send(NetworkMessage::reliable(&response));
                        last_sent_request.insert(player.transport_id, time.elapsed_seconds());
                    }
                }
            }
            ProfileEventType::Version(v) => {
                if let Ok((mut player, _)) = players.get_mut(ev.sender) {
                    if player.profile_version != v.profile_version {
                        player.profile_version = v.profile_version;
                    }
                } else {
                    warn!("profile version for unknown player {:?}", ev.sender);
                }
            }
            ProfileEventType::Response(r) => {
                if let Ok((mut player, maybe_profile)) = players.get_mut(ev.sender) {
                    let serialized_profile: SerializedProfile =
                        match serde_json::from_str(&r.serialized_profile) {
                            Ok(p) => p,
                            Err(e) => {
                                warn!("failed to parse profile: {e}");
                                continue;
                            }
                        };
                    let version = serialized_profile.version as u32;

                    // check/update profile version
                    if version < player.profile_version {
                        continue;
                    }
                    if version > player.profile_version {
                        player.profile_version = version;
                    }

                    let profile = UserProfile {
                        version,
                        content: serialized_profile,
                        base_url: r.base_url.clone(),
                    };

                    global_crdt.update_crdt(
                        SceneComponentId::PLAYER_IDENTITY_DATA,
                        CrdtType::LWW_ANY,
                        player.scene_id,
                        &PbPlayerIdentityData {
                            address: format!("{:#x}", player.address),
                            is_guest: !(profile.content.has_connected_web3.unwrap_or(false)),
                        },
                    );

                    cache.update(profile.clone());

                    if let Some(mut existing_profile) = maybe_profile {
                        if existing_profile.as_ref() != &profile {
                            *existing_profile = profile;
                        }
                    } else {
                        commands.entity(ev.sender).try_insert(profile);
                    }
                } else {
                    warn!("profile update for unknown player {:?}", ev.sender);
                }
            }
        }
    }

    last_sent_request.retain(|_, req_time| *req_time > time.elapsed_seconds() - 10.0);
}

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct UserProfile {
    pub version: u32,
    pub content: SerializedProfile,
    pub base_url: String,
}

impl UserProfile {
    pub fn is_female(&self) -> bool {
        self.content
            .avatar
            .body_shape
            .as_ref()
            .and_then(|s| s.rsplit(':').next())
            .map_or(true, |shape| shape.to_lowercase() == "basefemale")
    }
}

#[derive(Serialize)]
pub struct Deployment<'a> {
    version: &'a str,
    #[serde(rename = "type")]
    ty: &'a str,
    pointers: Vec<String>,
    timestamp: u128,
    content: Vec<TypedIpfsRef>,
    metadata: serde_json::Value,
}

async fn deploy_profile(
    ipfs: Arc<IpfsIo>,
    wallet: Wallet,
    mut profile: UserProfile,
    snapshots: Option<(Image, Image)>,
) -> Result<Option<(String, String)>, anyhow::Error> {
    let snap_details = if let Some((face, body)) = snapshots {
        let process = |img: Image| -> Result<_, anyhow::Error> {
            let img = img.clone().try_into_dynamic()?;
            let mut cursor = std::io::Cursor::new(Vec::default());
            img.write_to(&mut cursor, image::ImageFormat::Png)?;
            let bytes = cursor.into_inner();
            let hash = multihash_codetable::Code::Sha2_256.digest(bytes.as_slice());
            let cid = cid::Cid::new_v1(0x55, hash).to_string();
            Ok((bytes, cid))
        };

        let (face_bytes, face_cid) = process(face)?;
        let (body_bytes, body_cid) = process(body)?;

        profile.content.avatar.snapshots = Some(AvatarSnapshots {
            face256: face_cid.clone(),
            body: body_cid.clone(),
        });
        Some((face_bytes, face_cid, body_bytes, body_cid))
    } else {
        None
    };

    let unix_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();

    let snapshots = profile
        .content
        .avatar
        .snapshots
        .as_ref()
        .ok_or(anyhow!("no snapshots"))?
        .clone();

    let deployment = serde_json::to_string(&Deployment {
        version: "v3",
        ty: "profile",
        pointers: vec![profile.content.eth_address.clone()],
        timestamp: unix_time,
        content: vec![
            TypedIpfsRef {
                file: "body.png".to_owned(),
                hash: snapshots.body,
            },
            TypedIpfsRef {
                file: "face256.png".to_owned(),
                hash: snapshots.face256,
            },
        ],
        metadata: serde_json::json!({
            "avatars": [
                profile.content
            ]
        }),
    })?;

    let post = {
        let hash = multihash_codetable::Code::Sha2_256.digest(deployment.as_bytes());
        let cid = cid::Cid::new_v1(0x55, hash).to_string();
        let profile_chain = wallet.sign_message(cid.clone()).await?;

        let mut form_data = multipart::client::lazy::Multipart::new();
        form_data.add_text("entityId", cid.clone());
        for (key, data) in profile_chain.formdata() {
            form_data.add_text(key, data);
        }
        form_data.add_stream(
            cid,
            std::io::Cursor::new(deployment.into_bytes()),
            Option::<&str>::None,
            None,
        );

        if let Some((face_bytes, face_cid, body_bytes, body_cid)) = snap_details.clone() {
            debug!("deplying profile face: {face_cid}");
            form_data.add_stream(
                face_cid,
                std::io::Cursor::new(face_bytes),
                Option::<&str>::None,
                None,
            );
            debug!("deplying profile body: {body_cid}");
            form_data.add_stream(
                body_cid,
                std::io::Cursor::new(body_bytes),
                Option::<&str>::None,
                None,
            );
        }

        let mut prepared = form_data.prepare()?;
        let mut prepared_data = Vec::default();
        prepared.read_to_end(&mut prepared_data)?;

        let url = ipfs
            .entities_endpoint()
            .ok_or_else(|| anyhow!("no entities endpoint"))?;
        debug!("deploying to {url}");

        ipfs.client()
            .post(url)
            .header(
                "Content-Type",
                format!("multipart/form-data; boundary={}", prepared.boundary()),
            )
            .body(prepared_data)
    };

    let response = async_compat::Compat::new(async { post.send().await }).await?;

    match response.status() {
        StatusCode::OK => Ok(snap_details.map(|(_, face_cid, _, body_cid)| (face_cid, body_cid))),
        _ => Err(anyhow!(
            "bad response: {}: {}",
            response.status(),
            String::from_utf8_lossy(&response.bytes().await?)
        )),
    }
}

pub async fn get_remote_profile(
    address: Address,
    ipfs: std::sync::Arc<IpfsIo>,
    endpoint: Option<String>,
) -> Result<UserProfile, anyhow::Error> {
    let endpoint = match endpoint {
        Some(endpoint) => endpoint,
        None => ipfs.lambda_endpoint().ok_or(anyhow!("not connected"))?,
    };
    debug!("requesting profile from {}", endpoint);

    let response = async_compat::Compat::new(async {
        ipfs.client()
            .get(format!("{endpoint}/profiles/{address:#x}"))
            .send()
            .await
    })
    .await?;
    let mut content = response
        .json::<LambdaProfiles>()
        .await?
        .avatars
        .into_iter()
        .next()
        .ok_or(anyhow!("not found"))?;

    // clean up the lambda result
    if let Some(snapshots) = content.avatar.snapshots.as_mut() {
        if let Some(hash) = snapshots
            .body
            .rsplit_once('/')
            .map(|(_, hash)| hash.to_owned())
        {
            snapshots.body = hash;
        }
        if let Some(hash) = snapshots
            .face256
            .rsplit_once('/')
            .map(|(_, hash)| hash.to_owned())
        {
            snapshots.face256 = hash;
        }
    }

    let profile = UserProfile {
        version: content.version as u32,
        content,
        base_url: ipfs.contents_endpoint().unwrap_or_default(),
    };

    debug!("loaded profile: {profile:#?}");
    Ok(profile)
}
