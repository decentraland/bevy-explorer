use std::collections::HashMap;

use dcl_component::proto_components::common::Color3;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AvatarSnapshots {
    pub face256: String,
    pub body: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AvatarEmote {
    pub slot: u32,
    pub urn: String,
}

#[derive(Serialize, Deserialize, Copy, Clone, Debug, Default)]
pub struct AvatarColor {
    pub color: Color3,
}

impl PartialEq for AvatarColor {
    fn eq(&self, other: &Self) -> bool {
        self.color == other.color
    }
}

impl Eq for AvatarColor {}

impl AvatarColor {
    pub fn new(color: Color3) -> Self {
        Self { color }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AvatarWireFormat {
    pub name: Option<String>,
    pub body_shape: Option<String>,
    pub eyes: Option<AvatarColor>,
    pub hair: Option<AvatarColor>,
    pub skin: Option<AvatarColor>,
    pub wearables: Vec<String>,
    pub force_render: Option<Vec<String>>,
    pub emotes: Option<Vec<AvatarEmote>>,
    pub snapshots: Option<AvatarSnapshots>,
}

#[derive(Deserialize)]
pub struct LambdaProfiles {
    pub avatars: Vec<SerializedProfile>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SerializedProfile {
    pub user_id: Option<String>,
    pub name: String,
    pub version: i64,
    pub eth_address: String,
    pub has_claimed_name: bool,
    pub has_connected_web3: Option<bool>,
    pub avatar: AvatarWireFormat,
    pub extra_fields: HashMap<String, serde_json::Value>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfileJsonRef<'a> {
    pub user_id: &'a Option<String>,
    pub name: &'a String,
    pub version: i64,
    pub eth_address: &'a String,
    pub has_claimed_name: bool,
    pub has_connected_web3: Option<bool>,
    pub avatar: &'a AvatarWireFormat,
    #[serde(flatten)]
    pub extra_fields: &'a HashMap<String, serde_json::Value>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfileBinaryRef<'a> {
    pub user_id: &'a Option<String>,
    pub name: &'a String,
    pub version: i64,
    pub eth_address: &'a String,
    pub has_claimed_name: bool,
    pub has_connected_web3: Option<bool>,
    pub avatar: &'a AvatarWireFormat,
    pub extra_fields: &'a String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileJsonOwned {
    pub user_id: Option<String>,
    pub name: String,
    pub version: i64,
    pub eth_address: String,
    pub has_claimed_name: bool,
    pub has_connected_web3: Option<bool>,
    pub avatar: AvatarWireFormat,
    #[serde(flatten)]
    pub extra_fields: HashMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileBinaryOwned {
    pub user_id: Option<String>,
    pub name: String,
    pub version: i64,
    pub eth_address: String,
    pub has_claimed_name: bool,
    pub has_connected_web3: Option<bool>,
    pub avatar: AvatarWireFormat,
    pub extra_fields: String,
}

impl Serialize for SerializedProfile {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            // JSON -> use flattened reference shadow
            let shadow = ProfileJsonRef {
                user_id: &self.user_id,
                name: &self.name,
                version: self.version,
                eth_address: &self.eth_address,
                has_claimed_name: self.has_claimed_name,
                has_connected_web3: self.has_connected_web3,
                avatar: &self.avatar,
                extra_fields: &self.extra_fields,
            };
            shadow.serialize(serializer)
        } else {
            // bincode -> use nested reference shadow
            let shadow = ProfileBinaryRef {
                user_id: &self.user_id,
                name: &self.name,
                version: self.version,
                eth_address: &self.eth_address,
                has_claimed_name: self.has_claimed_name,
                has_connected_web3: self.has_connected_web3,
                avatar: &self.avatar,
                extra_fields: &serde_json::to_string(&self.extra_fields).unwrap_or_default(),
            };
            shadow.serialize(serializer)
        }
    }
}

impl<'de> Deserialize<'de> for SerializedProfile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            // JSON -> deserialize into flattened shadow
            let shadow = ProfileJsonOwned::deserialize(deserializer)?;
            Ok(SerializedProfile {
                user_id: shadow.user_id,
                name: shadow.name,
                version: shadow.version,
                eth_address: shadow.eth_address,
                has_claimed_name: shadow.has_claimed_name,
                has_connected_web3: shadow.has_connected_web3,
                avatar: shadow.avatar,
                extra_fields: shadow.extra_fields,
            })
        } else {
            // bincode -> deserialize into nested shadow
            let shadow = ProfileBinaryOwned::deserialize(deserializer)?;
            Ok(SerializedProfile {
                user_id: shadow.user_id,
                name: shadow.name,
                version: shadow.version,
                eth_address: shadow.eth_address,
                has_claimed_name: shadow.has_claimed_name,
                has_connected_web3: shadow.has_connected_web3,
                avatar: shadow.avatar,
                extra_fields: serde_json::from_str(&shadow.extra_fields).unwrap_or_default(),
            })
        }
    }
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
                \"emotes\":[
                    {\"slot\": 0, \"urn\": \"handsair\" },
                    {\"slot\": 1, \"urn\": \"wave\" },
                    {\"slot\": 2, \"urn\": \"fistpump\" },
                    {\"slot\": 3, \"urn\": \"dance\" },
                    {\"slot\": 4, \"urn\": \"raisehand\" },
                    {\"slot\": 5, \"urn\": \"clap\" },
                    {\"slot\": 6, \"urn\": \"money\" },
                    {\"slot\": 7, \"urn\": \"kiss\" },
                    {\"slot\": 8, \"urn\": \"headexplode\" },
                    {\"slot\": 9, \"urn\": \"shrug\" }
                ],
               \"snapshots\": {
                    \"face256\":\"\",
                    \"body\":\"\"
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

        //                     {\"slot\": 1, \"urn\": \"urn:decentraland:matic:collections-v2:0x9087f96750c4e7607454c67c4f0bcf357ae62a46:2\" }

        Self {
            user_id: Default::default(),
            name: "Bevy_User".to_string(),
            version: 1,
            eth_address: "0x0000000000000000000000000000000000000000".to_owned(),
            has_claimed_name: Default::default(),
            has_connected_web3: Default::default(),
            avatar,
            extra_fields: HashMap::from_iter([
                (
                    "description".to_owned(),
                    serde_json::to_value(String::default()).unwrap(),
                ),
                (
                    "tutorialStep".to_owned(),
                    serde_json::to_value(0u32).unwrap(),
                ),
            ]),
        }
    }
}
