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
/// Salt for the GPU pipeline-cache key. `precomputed_shader_hash` only sees the
/// shader *files*, so a change that alters generated shaders without touching
/// them (e.g. the naga_oil override-emit-order fix) is invisible to it. Bump
/// this to force every client to drop its stale cached pipelines on next load.
#[cfg(target_arch = "wasm32")]
const GPU_CACHE_SALT: u32 = 1;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn gpu_cache_hash() -> String {
    format!("{}-{}", precomputed_shader_hash(), GPU_CACHE_SALT)
}
