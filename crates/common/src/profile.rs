use dcl_component::proto_components::common::Color3;
use serde::{Deserialize, Serialize};

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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SerializedProfile {
    pub user_id: Option<String>,
    pub name: String,
    pub description: String,
    pub version: i64,
    pub eth_address: String,
    pub tutorial_step: u32,
    pub email: Option<String>,
    pub blocked: Option<Vec<String>>,
    pub muted: Option<Vec<String>>,
    pub interests: Option<Vec<String>>,
    pub has_claimed_name: bool,
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
                    \"face256\":\"bafkreifxucgpg7w2rnnfevjzvlokojxnxqh7uvxhe2nwwyks4xr4unx4bu\",
                    \"body\":\"bafkreicdr2mdferawy7bwl3vwitqg37sohjmkt6zsaqbhlmxwprcquaacy\"
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
