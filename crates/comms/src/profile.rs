use bevy::{prelude::*, utils::HashMap};
use ethers_core::types::Address;
use serde::{Deserialize, Serialize};

use common::structs::{AppConfig, PrimaryUser};
use common::util::{AsH160, TryInsertEx};
use dcl_component::proto_components::{common::Color3, kernel::comms::rfc4};
use wallet::Wallet;
use super::{
    global_crdt::{process_transport_updates, ForeignPlayer, ProfileEvent, ProfileEventType},
    NetworkMessage, Transport,
};

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
        let wallet = app.world.resource::<Wallet>();

        let config = &app.world.resource::<AppConfig>();
        let current_content = serde_json::from_str::<SerializedProfile>(&config.profile_content)
            .unwrap_or(SerializedProfile::default());

        let user_profile = UserProfile {
            version: config.profile_version,
            content: SerializedProfile {
                user_id: Some(format!("{:#x}", wallet.address())),
                ..current_content
            },
            base_url: config.profile_base_url.clone(),
        };
        app.insert_resource(CurrentUserProfile(user_profile));
    }
}

pub fn setup_primary_profile(
    mut commands: Commands,
    player: Query<(Entity, Option<&UserProfile>), With<PrimaryUser>>,
    current_profile: Res<CurrentUserProfile>,
    transports: Query<&Transport>,
) {
    if let Ok((player, maybe_profile)) = player.get_single() {
        if maybe_profile.is_none() || current_profile.is_changed() {
            // update component
            commands
                .entity(player)
                .try_insert(current_profile.0.clone());

            // send over network
            debug!(
                "sending profile new version {:?}",
                current_profile.0.version
            );
            let response = rfc4::Packet {
                message: Some(rfc4::packet::Message::ProfileResponse(
                    rfc4::ProfileResponse {
                        serialized_profile: serde_json::to_string(&current_profile.0.content)
                            .unwrap(),
                        base_url: current_profile.0.base_url.clone(),
                    },
                )),
            };
            for transport in &transports {
                let _ = transport
                    .sender
                    .try_send(NetworkMessage::reliable(&response));
            }

            // store to app config
            let mut config: AppConfig = std::fs::read("config.json")
                .ok()
                .and_then(|f| serde_json::from_slice(&f).ok())
                .unwrap_or(Default::default());
            config.profile_version = current_profile.0.version;
            config.profile_content = serde_json::to_string(&current_profile.0.content).unwrap();
            config.profile_base_url = current_profile.0.base_url.clone();
            if let Err(e) = std::fs::write("config.json", serde_json::to_string(&config).unwrap()) {
                warn!("failed to write to config: {e}");
            }
        }
    }
}

#[derive(Resource)]
pub struct CurrentUserProfile(pub UserProfile);

fn request_missing_profiles(
    missing_profiles: Query<&mut ForeignPlayer, Without<UserProfile>>,
    stale_profiles: Query<(&mut ForeignPlayer, &UserProfile)>,
    mut requested: Local<HashMap<Address, f32>>,
    transports: Query<&Transport>,
    time: Res<Time>,
) {
    let mut last_requested = std::mem::take(&mut *requested);

    for player in missing_profiles.iter().chain(
        stale_profiles
            .iter()
            .filter(|(player, profile)| player.profile_version > profile.version)
            .map(|(player, _)| player),
    ) {
        if let Some((address, req_time)) = last_requested.remove_entry(&player.address) {
            if time.elapsed_seconds() - req_time < 10.0 {
                requested.insert(address, req_time);
                continue;
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
) {
    for ev in events.iter() {
        match &ev.event {
            ProfileEventType::Request(r) => {
                if let Some(req_address) = r.address.as_h160() {
                    if req_address == wallet.address() {
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

                        debug!("sending my profile");
                        let response = rfc4::Packet {
                            message: Some(rfc4::packet::Message::ProfileResponse(
                                rfc4::ProfileResponse {
                                    serialized_profile: serde_json::to_string(
                                        &current_profile.0.content,
                                    )
                                    .unwrap(),
                                    base_url: current_profile.0.base_url.clone(),
                                },
                            )),
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
                    player.profile_version = v.profile_version;
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
                        return;
                    }
                    if version > player.profile_version {
                        player.profile_version = version;
                    }

                    let profile = UserProfile {
                        version,
                        content: serialized_profile,
                        base_url: r.base_url.clone(),
                    };

                    if let Some(mut existing_profile) = maybe_profile {
                        *existing_profile = profile;
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

#[derive(Serialize, Deserialize, Clone)]
pub struct AvatarSnapshots {
    pub face256: String,
    pub body: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AvatarEmote {
    pub slot: u32,
    pub urn: String,
}

#[derive(Serialize, Deserialize, Copy, Clone)]
pub struct AvatarColor {
    pub color: Color3,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AvatarWireFormat {
    pub name: Option<String>,
    #[serde(rename = "bodyShape")]
    pub body_shape: Option<String>,
    pub eyes: Option<AvatarColor>,
    pub hair: Option<AvatarColor>,
    pub skin: Option<AvatarColor>,
    pub wearables: Vec<String>,
    pub emotes: Option<Vec<AvatarEmote>>,
    pub snapshots: Option<AvatarSnapshots>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SerializedProfile {
    #[serde(rename = "userId")]
    pub user_id: Option<String>,
    pub name: String,
    pub description: String,
    pub version: i64,
    #[serde(rename = "ethAddress")]
    pub eth_address: String,
    #[serde(rename = "tutorialStep")]
    pub tutorial_step: u32,
    pub email: Option<String>,
    pub blocked: Option<Vec<String>>,
    pub muted: Option<Vec<String>>,
    pub interests: Option<Vec<String>>,
    #[serde(rename = "hasClaimedName")]
    pub has_claimed_name: Option<bool>,
    #[serde(rename = "hasConnectedWeb3")]
    pub has_connected_web3: Option<bool>,
    pub avatar: AvatarWireFormat,
}

impl Default for SerializedProfile {
    fn default() -> Self {
        let avatar = serde_json::from_str("
            {
                \"bodyShape\":\"urn:decentraland:off-chain:base-avatars:BaseFemale\",
                \"wearables\":[
                    \"urn:decentraland:off-chain:base-avatars:f_sweater\",
                    \"urn:decentraland:off-chain:base-avatars:f_jeans\",
                    \"urn:decentraland:off-chain:base-avatars:bun_shoes\",
                    \"urn:decentraland:off-chain:base-avatars:standard_hair\",
                    \"urn:decentraland:off-chain:base-avatars:f_eyes_01\",
                    \"urn:decentraland:off-chain:base-avatars:f_eyebrows_00\",
                    \"urn:decentraland:off-chain:base-avatars:f_mouth_00\"
                ],
                \"snapshots\": {
                    \"face256\":\"QmSqZ2npVD4RLdqe17FzGCFcN29RfvmqmEd2FcQUctxaKk\",
                    \"body\":\"QmSav1o6QK37Jj1yhbmhYk9MJc6c2H5DWbWzPVsg9JLYfF\"
                },
                \"eyes\":{
                    \"color\":{\"r\":0.3,\"g\":0.2235294133424759,\"b\":0.99,\"a\":1}
                },
                \"hair\":{
                    \"color\":{\"r\":0.5960784554481506,\"g\":0.37254902720451355,\"b\":0.21568627655506134,\"a\":1}
                },
                \"skin\":{
                    \"color\":{\"r\":0.4901960790157318,\"g\":0.364705890417099,\"b\":0.27843138575553894,\"a\":1}
                }
            }
        ").unwrap();

        Self {
            user_id: Default::default(),
            name: "Bevy User".to_string(),
            description: Default::default(),
            version: 1,
            eth_address: "0x0000000000000000000000000000000000000000".to_owned(),
            tutorial_step: Default::default(),
            email: Default::default(),
            blocked: Default::default(),
            muted: Default::default(),
            interests: Default::default(),
            has_claimed_name: Default::default(),
            has_connected_web3: Default::default(),
            avatar,
        }
    }
}

#[derive(Component, Serialize, Deserialize, Clone)]
pub struct UserProfile {
    pub version: u32,
    pub content: SerializedProfile,
    pub base_url: String,
}
