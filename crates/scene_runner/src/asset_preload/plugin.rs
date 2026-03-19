use std::{convert::Infallible, path::PathBuf};

use bevy::{
    asset::AssetLoader,
    ecs::{component::HookContext, entity::EntityHashSet, world::DeferredWorld},
    platform::collections::HashMap,
    prelude::*,
};
use dcl::interface::ComponentPosition;
use dcl_component::{proto_components::sdk::components::PbAssetLoad, SceneComponentId};
use ipfs::ipfs_path::{IpfsPath, IpfsType};

use crate::{
    asset_preload::{AssetLoad, PreloadAsset},
    renderer_context::RendererSceneContext,
    update_world::AddCrdtInterfaceExt,
    ContainerEntity,
};

pub struct AssetPreloadPlugin;

impl Plugin for AssetPreloadPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AssetPreloadBackReference>();

        app.init_asset::<PreloadAsset>();
        app.init_asset_loader::<PreloadAssetLoader>();

        app.add_crdt_lww_component::<PbAssetLoad, AssetLoad>(
            SceneComponentId::ASSET_LOAD,
            ComponentPosition::EntityOnly,
        );

        app.world_mut()
            .register_component_hooks::<AssetLoad>()
            .on_insert(asset_preload_on_insert)
            .on_replace(asset_preload_on_replace);
    }
}

/// Mapping between [`PreloadAsset`] [`Handle`] to entities requesting them
#[derive(Default, Resource, Deref, DerefMut)]
struct AssetPreloadBackReference {
    assets: HashMap<Handle<PreloadAsset>, EntityHashSet>,
}

fn asset_preload_on_insert(mut deferred_world: DeferredWorld, hook_context: HookContext) {
    let entity = hook_context.entity;

    let asset_server = deferred_world.resource::<AssetServer>().clone();

    let Some(asset_load) = deferred_world.get::<AssetLoad>(entity) else {
        unreachable!("AssetLoad must be available on its hook");
    };
    debug!(
        "Entity {entity} is requesting assets {:?}",
        asset_load.assets
    );

    let Some(container_entity) = deferred_world.get::<ContainerEntity>(entity) else {
        panic!("Entity with AssetLoad does not have ContainerEntity");
    };

    let Some(renderer_scene_context) =
        deferred_world.get::<RendererSceneContext>(container_entity.root)
    else {
        panic!("Root of AssetLoad does not have RendererSceneContext");
    };
    let scene_hash = &renderer_scene_context.hash;

    let asset_preload_handles = asset_load
        .iter()
        .map(|file_path| {
            let ipfs_path = IpfsPath::new(IpfsType::new_content_file(
                scene_hash.to_owned(),
                file_path.to_owned(),
            ));
            asset_server.load(PathBuf::from(&ipfs_path))
        })
        .collect::<Vec<Handle<PreloadAsset>>>();

    let mut asset_preload_counter = deferred_world.resource_mut::<AssetPreloadBackReference>();

    for handle in asset_preload_handles {
        asset_preload_counter
            .entry(handle)
            .and_modify(|set| {
                debug_assert!(set.insert(entity));
            })
            .or_insert_with(|| {
                let mut set = EntityHashSet::new();
                set.insert(entity);
                set
            });
    }
}

fn asset_preload_on_replace(mut deferred_world: DeferredWorld, hook_context: HookContext) {
    let entity = hook_context.entity;

    let asset_server = deferred_world.resource::<AssetServer>().clone();

    let Some(asset_load) = deferred_world.get::<AssetLoad>(entity) else {
        unreachable!("AssetLoad must be available on its hook");
    };

    let Some(container_entity) = deferred_world.get::<ContainerEntity>(entity) else {
        panic!("Entity with AssetLoad does not have ContainerEntity");
    };

    let Some(renderer_scene_context) =
        deferred_world.get::<RendererSceneContext>(container_entity.root)
    else {
        panic!("Root of AssetLoad does not have RendererSceneContext");
    };
    let scene_hash = &renderer_scene_context.hash;

    let Some(asset_preload_handles) = asset_load
        .iter()
        .map(|file_path| {
            let ipfs_path = IpfsPath::new(IpfsType::new_content_file(
                scene_hash.to_owned(),
                file_path.to_owned(),
            ));
            asset_server.get_handle(PathBuf::from(&ipfs_path))
        })
        .collect::<Option<Vec<_>>>()
    else {
        unreachable!("All assets in AssetLoad must have a handle at this point.");
    };

    let mut asset_preload_counter = deferred_world.resource_mut::<AssetPreloadBackReference>();

    for handle in asset_preload_handles {
        let Some(set) = asset_preload_counter.get_mut(&handle) else {
            unreachable!("All handles of AssetLoad must be present on AssetPreloadBackReferenece.");
        };

        debug_assert!(set.remove(&entity));
        if set.is_empty() {
            asset_preload_counter.remove(&handle);
        } else {
            *counter -= 1;
        }
    }
}

#[derive(Default)]
struct PreloadAssetLoader;

impl AssetLoader for PreloadAssetLoader {
    type Asset = PreloadAsset;
    type Settings = ();
    type Error = Infallible;

    async fn load(
        &self,
        _reader: &mut dyn bevy::asset::io::Reader,
        _settings: &Self::Settings,
        load_context: &mut bevy::asset::LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        debug!("Preloaded {}", load_context.path().display());
        Ok(PreloadAsset)
    }

    fn extensions(&self) -> &[&str] {
        &[]
    }
}
