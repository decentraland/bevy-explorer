use bevy::{asset::io::embedded::EmbeddedAssetRegistry, prelude::*};

include!(concat!(env!("OUT_DIR"), "/generated_asset_embedding.rs"));

pub struct EmbedAssetsPlugin;

impl Plugin for EmbedAssetsPlugin {
    fn build(&self, app: &mut App) {
        let embedded = app.world_mut().resource_mut::<EmbeddedAssetRegistry>();
        embed_assets(embedded.into_inner());
    }
}

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn gpu_cache_hash() -> String {
    format!("{}", precomputed_shader_hash())
}
