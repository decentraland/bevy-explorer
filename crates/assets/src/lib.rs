use std::ops::DerefMut;

use bevy::{
    asset::io::embedded::EmbeddedAssetRegistry, platform::collections::HashSet, prelude::*,
};

include!(concat!(env!("OUT_DIR"), "/generated_asset_embedding.rs"));

pub struct EmbedAssetsPlugin;

impl Plugin for EmbedAssetsPlugin {
    fn build(&self, app: &mut App) {
        let embedded = app.world_mut().resource_mut::<EmbeddedAssetRegistry>();
        embed_assets(embedded.into_inner());

        app.add_systems(
            Update,
            disable_anisotropy.run_if(on_event::<AssetEvent<StandardMaterial>>),
        );
    }
}

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn gpu_cache_hash() -> String {
    format!("{}", precomputed_shader_hash())
}

fn disable_anisotropy(
    mut asset_events: EventReader<AssetEvent<StandardMaterial>>,
    mut standard_materials: ResMut<Assets<StandardMaterial>>,
    mut cache_a: Local<HashSet<AssetId<StandardMaterial>>>,
    mut cache_b: Local<HashSet<AssetId<StandardMaterial>>>,
) {
    for event in asset_events.read() {
        if let AssetEvent::Added { id }
        | AssetEvent::Modified { id }
        | AssetEvent::LoadedWithDependencies { id } = event
        {
            cache_b.insert(*id);

            if !cache_a.contains(id) {
                let Some(material) = standard_materials.get_mut(*id) else {
                    unreachable!("Invalid id {id}.");
                };

                debug!("Disabling anisotropy from {id}.");
                material.anisotropy_strength = 0.;
            }
        }
    }

    std::mem::swap(cache_a.deref_mut(), cache_b.deref_mut());
    cache_b.clear();
}
