use std::str::FromStr;

use bevy::{prelude::*, utils::{HashMap, HashSet}};
use serde::{Serialize, Deserialize};
use urn::Urn;

pub mod base_wearables;

use crate::{ipfs::{ActiveEntityTask, IpfsLoaderExt}, util::TaskExt, comms::{global_crdt::ForeignPlayer, profile::UserProfile}};

pub struct AvatarPlugin;

impl Plugin for AvatarPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WearablePointers>();
        app.add_system(load_base_wearables);
        app.add_system(update_avatars);
    }
}

#[derive(Resource, Default, Debug)]
pub struct WearablePointers(HashMap<Urn, String>);

pub struct WearableManifests(HashMap<String, WearableManifest>);

pub struct WearableManifest {

}

#[derive(Deserialize, Debug)]
pub struct WearableMeta {

}

fn load_base_wearables(
    mut once: Local<bool>,
    mut task: Local<Option<ActiveEntityTask>>,
    mut wearable_pointers: ResMut<WearablePointers>,
    asset_server: Res<AssetServer>,
) {
    if *once || asset_server.active_endpoint().is_none() {
        return;
    }

    match *task {
        None => {
            let pointers = base_wearables::base_wearables();
            *task = Some(asset_server.ipfs().active_entities(&pointers));
        }
        Some(ref mut active_task) => {
            match active_task.complete() {
                None => (),
                Some(Err(e)) => warn!("failed to acquire base wearables: {e}"),
                Some(Ok(active_entities)) => {
                    for entity in active_entities {
                        for pointer in entity.pointers {
                            match Urn::from_str(&pointer) {
                                Ok(urn) => { wearable_pointers.0.insert(urn, entity.id.clone()); },
                                Err(e) => { warn!("failed to parse wearable urn: {e}"); },
                            };
                        }
                    }
                    *task = None;
                    *once = true;
                    println!("found items");
                    println!("{wearable_pointers:?}");
                }
            }
        }
    }
}

#[derive(Component)]
struct AvatarRenderEntity;

fn update_avatars(
    mut commands: Commands,
    updated_players: Query<(&ForeignPlayer, &UserProfile, &Children), Changed<UserProfile>>,
    _avatar_entities: Query<&AvatarRenderEntity>,
) {
    for (_player, profile, children) in &updated_players {
        for child in children.iter() {
            if let Some(commands) = commands.get_entity(*child) {
                commands.despawn_recursive();
            }
        }

        let avatar = &profile.content.avatar;
        let _base_shape_urn = &avatar.body_shape;
    }
}

#[derive(Component)]
struct PendingAvatarComponents(HashSet<Urn>);

#[derive(Serialize, Deserialize)]
struct AvatarColorInner {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

#[derive(Serialize, Deserialize)]
struct AvatarColor {
    pub color: AvatarColorInner,
}

#[derive(Serialize, Deserialize)]
pub struct AvatarSnapshots {
    pub face256: String,
    pub body: String,
}

#[derive(Serialize, Deserialize)]
pub struct Avatar {
    #[serde(rename="bodyShape")]
    body_shape: String,
    eyes: AvatarColor,
    hair: AvatarColor,
    skin: AvatarColor,
    wearables: Vec<String>,
    emotes: Option<serde_json::Value>,
    snapshots: Option<AvatarSnapshots>,
}