use std::{convert::Infallible, io::ErrorKind, path::PathBuf};

use bevy::{
    asset::{io::AssetReaderError, AssetLoadError, AssetLoader, RecursiveDependencyLoadState},
    ecs::relationship::Relationship,
    prelude::*,
};
use common::structs::MonotonicTimestamp;
use dcl::interface::{ComponentPosition, CrdtType};
use dcl_component::{
    proto_components::sdk::components::{
        common::LoadingState, PbAssetLoad, PbAssetLoadLoadingState,
    },
    SceneComponentId,
};
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
        app.init_asset::<PreloadAsset>();
        app.init_asset_loader::<PreloadAssetLoader>();

        app.init_resource::<MonotonicTimestamp<PbAssetLoadLoadingState>>();

        app.add_crdt_lww_component::<PbAssetLoad, AssetLoad>(
            SceneComponentId::ASSET_LOAD,
            ComponentPosition::EntityOnly,
        );

        app.add_observer(asset_load_on_insert);
        app.add_observer(asset_load_on_replace);

        app.add_systems(Update, verify_preload_state);
    }
}

#[derive(Component)]
#[relationship(relationship_target = Preloader)]
struct PreloadedAssetOf(Entity);

#[derive(Component)]
#[relationship_target(relationship = PreloadedAssetOf, linked_spawn)]
struct Preloader(Vec<Entity>);

#[derive(Component)]
struct PreloadedAsset {
    file_path: String,
    handle: Handle<PreloadAsset>,
}

#[derive(Component)]
struct LoadingPreloadedAsset;

fn asset_load_on_insert(
    trigger: Trigger<OnInsert, AssetLoad>,
    mut commands: Commands,
    asset_loads: Query<(&AssetLoad, Option<&ContainerEntity>)>,
    mut renderer_scene_contexts: Query<&mut RendererSceneContext>,
    asset_server: Res<AssetServer>,
    timestamp: Res<MonotonicTimestamp<PbAssetLoadLoadingState>>,
) {
    let entity = trigger.target();

    let Ok((asset_load, maybe_container_entity)) = asset_loads.get(entity) else {
        #[cfg(debug_assertions)]
        unreachable!("AssetLoad must be available to its observers.");
        #[cfg(not(debug_assertions))]
        {
            error!("AssetLoad must be available to its observers.");
            return;
        }
    };
    let Some(container_entity) = maybe_container_entity else {
        #[cfg(debug_assertions)]
        panic!("AssetLoad entity did not have ContainerEntity.");
        #[cfg(not(debug_assertions))]
        {
            error!("AssetLoad entity did not have ContainerEntity.");
            return;
        }
    };
    debug!(
        "Entity {} on {} requested assets {:?}.",
        entity, container_entity.root, asset_load.assets
    );

    let Ok(mut renderer_scene_context) = renderer_scene_contexts.get_mut(container_entity.root)
    else {
        #[cfg(debug_assertions)]
        panic!("Root of AssetLoad does not contain RendererSceneContext.");
        #[cfg(not(debug_assertions))]
        {
            error!("Root of AssetLoad does not contain RendererSceneContext.");
            return;
        }
    };

    for file_path in &asset_load.assets {
        let ipfs_path = IpfsPath::new(IpfsType::new_content_file(
            renderer_scene_context.hash.to_owned(),
            file_path.to_owned(),
        ));
        let handle: Handle<PreloadAsset> = asset_server.load(PathBuf::from(&ipfs_path));

        commands.spawn((
            PreloadedAsset {
                file_path: file_path.to_owned(),
                handle,
            },
            PreloadedAssetOf(entity),
            LoadingPreloadedAsset,
        ));

        let event = PbAssetLoadLoadingState {
            current_state: LoadingState::Loading as i32,
            asset: file_path.to_owned(),
            timestamp: timestamp.next_timestamp(),
        };
        renderer_scene_context.update_crdt(
            SceneComponentId::ASSET_LOAD_LOADING_STATE,
            CrdtType::GO_ANY,
            container_entity.container_id,
            &event,
        );
    }
}

fn asset_load_on_replace(
    trigger: Trigger<OnReplace, AssetLoad>,
    mut commands: Commands,
    asset_loads: Query<(&AssetLoad, Option<&ContainerEntity>)>,
) {
    let entity = trigger.target();

    let Ok((asset_load, maybe_container_entity)) = asset_loads.get(entity) else {
        #[cfg(debug_assertions)]
        unreachable!("AssetLoad must be available to its observers.");
        #[cfg(not(debug_assertions))]
        {
            error!("AssetLoad must be available to its observers.");
            return;
        }
    };
    let Some(container_entity) = maybe_container_entity else {
        #[cfg(debug_assertions)]
        panic!("AssetLoad entity did not have ContainerEntity.");
        #[cfg(not(debug_assertions))]
        {
            error!("AssetLoad entity did not have ContainerEntity.");
            return;
        }
    };
    debug!(
        "Entity {} on {} no longer requires assets {:?}.",
        entity, container_entity.root, asset_load.assets
    );

    commands.entity(entity).queue_handled(
        |mut entity: EntityWorldMut| {
            entity.despawn_related::<Preloader>();
        },
        // This might happen on despawn, and if it is the case, just leave it be
        bevy::ecs::error::ignore,
    );
}

fn verify_preload_state(
    mut commands: Commands,
    preloaded_assets: Populated<
        (Entity, &PreloadedAsset, &PreloadedAssetOf),
        With<LoadingPreloadedAsset>,
    >,
    asset_loads: Query<&ContainerEntity, With<AssetLoad>>,
    mut renderer_scene_contexts: Query<&mut RendererSceneContext>,
    asset_server: Res<AssetServer>,
    timestamp: Res<MonotonicTimestamp<PbAssetLoadLoadingState>>,
) {
    for (entity, preloaded_asset, preloaded_asset_of) in preloaded_assets.into_inner() {
        let Ok(container_entity) = asset_loads.get(preloaded_asset_of.get()) else {
            #[cfg(debug_assertions)]
            panic!("Could not get the AssetLoad of a PreloadedAsset.");
            #[cfg(not(debug_assertions))]
            {
                error!("Could not get the AssetLoad of a PreloadedAsset.");
                continue;
            }
        };
        let Ok(mut renderer_scene_context) = renderer_scene_contexts.get_mut(container_entity.root)
        else {
            #[cfg(debug_assertions)]
            panic!("Root of AssetLoad does not contain RendererSceneContext.");
            #[cfg(not(debug_assertions))]
            {
                error!("Root of AssetLoad does not contain RendererSceneContext.");
                continue;
            }
        };

        match asset_server.get_recursive_dependency_load_state(preloaded_asset.handle.id()) {
            Some(
                RecursiveDependencyLoadState::NotLoaded | RecursiveDependencyLoadState::Loading,
            ) => (),
            Some(RecursiveDependencyLoadState::Loaded) => {
                let event = PbAssetLoadLoadingState {
                    current_state: LoadingState::Finished as i32,
                    asset: preloaded_asset.file_path.to_owned(),
                    timestamp: timestamp.next_timestamp(),
                };
                renderer_scene_context.update_crdt(
                    SceneComponentId::ASSET_LOAD_LOADING_STATE,
                    CrdtType::GO_ANY,
                    container_entity.container_id,
                    &event,
                );
                commands.entity(entity).remove::<LoadingPreloadedAsset>();
            }
            Some(RecursiveDependencyLoadState::Failed(err)) => {
                let loading_state = match err.as_ref() {
                    AssetLoadError::AssetReaderError(AssetReaderError::NotFound(_)) => {
                        LoadingState::NotFound
                    }
                    AssetLoadError::AssetReaderError(AssetReaderError::Io(io)) => {
                        let message = io.to_string();
                        if io.kind() == ErrorKind::Other && message.starts_with("w: file not found")
                        {
                            LoadingState::NotFound
                        } else {
                            LoadingState::FinishedWithError
                        }
                    }
                    _ => LoadingState::FinishedWithError,
                };

                let event = PbAssetLoadLoadingState {
                    current_state: loading_state as i32,
                    asset: preloaded_asset.file_path.to_owned(),
                    timestamp: timestamp.next_timestamp(),
                };
                renderer_scene_context.update_crdt(
                    SceneComponentId::ASSET_LOAD_LOADING_STATE,
                    CrdtType::GO_ANY,
                    container_entity.container_id,
                    &event,
                );
                commands.entity(entity).remove::<LoadingPreloadedAsset>();
            }
            None => {
                #[cfg(debug_assertions)]
                panic!("Preload asset handle not found in asset server.");
                #[cfg(not(debug_assertions))]
                {
                    error!("Preload asset handle not found in asset server.");
                    continue;
                }
            }
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
