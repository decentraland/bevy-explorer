pub mod base_wearables;
pub mod emotes;
pub mod urn;
pub mod wearables;

use bevy::prelude::Plugin;

pub use emotes::*;
use wearables::WearablePlugin;

pub struct CollectiblesPlugin;

impl Plugin for CollectiblesPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_plugins((EmotesPlugin, WearablePlugin));
    }
}

pub trait CollectibleType {
    fn base_collection() -> Option<&'static str>;
}
