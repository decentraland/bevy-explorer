use bevy::prelude::*;

pub mod base_wearables;

use crate::ipfs::{ActiveEntityTask, IpfsLoaderExt};

pub struct AvatarPlugin;

impl Plugin for AvatarPlugin {
    fn build(&self, app: &mut App) {
        app.add_system(load_base_wearables);
    }
}

fn load_base_wearables(
    mut once: Local<bool>,
    mut task: Local<Option<ActiveEntityTask>>,
    asset_server: Res<AssetServer>,
) {
    if *once {
        println!("all good!");
        return;
    }

    match *task {
        None => {
            let pointers = base_wearables::BASE_WEARABLES
                .iter()
                .map(ToString::to_string)
                .collect();
            *task = Some(asset_server.ipfs().active_entities(&pointers));
        }
        Some(ref mut active_task) => {
            if active_task.is_finished() {
                *task = None;
                *once = true;
                println!("found items");
            }
        }
    }
}
