pub mod plugin;

use bevy::prelude::*;
use dcl_component::proto_components::sdk::components::PbAssetLoad;

#[derive(Asset, TypePath)]
pub struct PreloadAsset;

#[derive(Debug, Component, Deref, DerefMut)]
#[component(immutable)]
pub struct AssetLoad {
    assets: Vec<String>,
}

impl From<PbAssetLoad> for AssetLoad {
    fn from(value: PbAssetLoad) -> Self {
        Self {
            assets: value.assets,
        }
    }
}
