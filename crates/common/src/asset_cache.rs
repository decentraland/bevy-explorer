use std::hash::Hash;

use bevy::{platform::collections::HashMap, prelude::*};

/// caches assets by key so identical assets are shared instead of re-added.
/// entries are weak; `clean_asset_cache` drops keys whose assets have been unloaded.
#[derive(Resource)]
pub struct AssetCache<K: Send + Sync + 'static, A: Asset>(HashMap<K, AssetId<A>>);

impl<K: Send + Sync + 'static, A: Asset> Default for AssetCache<K, A> {
    fn default() -> Self {
        Self(HashMap::default())
    }
}

impl<K: Eq + Hash + Send + Sync + 'static, A: Asset> AssetCache<K, A> {
    pub fn get_or_add(
        &mut self,
        key: K,
        assets: &mut Assets<A>,
        create: impl FnOnce() -> A,
    ) -> Handle<A> {
        if let Some(handle) = self
            .0
            .get(&key)
            .and_then(|id| assets.get_strong_handle(*id))
        {
            return handle;
        }
        let handle = assets.add(create());
        self.0.insert(key, handle.id());
        handle
    }
}

pub fn clean_asset_cache<K: Send + Sync + 'static, A: Asset>(
    mut cache: ResMut<AssetCache<K, A>>,
    assets: Res<Assets<A>>,
) {
    cache.0.retain(|_, id| assets.contains(*id));
}
