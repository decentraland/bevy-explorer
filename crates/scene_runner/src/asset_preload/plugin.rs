use std::path::PathBuf;

use bevy::{
    asset::{
        io::AssetReaderError, AssetLoadError, LoadedUntypedAsset, RecursiveDependencyLoadState,
    },
    ecs::relationship::Relationship,
    prelude::*,
};
use common::{debug_panic, structs::MonotonicTimestamp};
use dcl::interface::{ComponentPosition, CrdtType};
use dcl_component::{
    proto_components::sdk::components::{
        common::LoadingState, PbAssetLoad, PbAssetLoadLoadingState,
    },
    SceneComponentId,
};
use ipfs::ipfs_path::{IpfsPath, IpfsType};

use crate::{
    asset_preload::AssetLoad, renderer_context::RendererSceneContext,
    update_world::AddCrdtInterfaceExt, ContainerEntity,
};

pub struct AssetPreloadPlugin;

impl Plugin for AssetPreloadPlugin {
    fn build(&self, app: &mut App) {
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
    handle: Handle<LoadedUntypedAsset>,
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
        debug_panic!("AssetLoad must be available to its observers.");
    };
    let Some(container_entity) = maybe_container_entity else {
        debug_panic!("AssetLoad entity did not have ContainerEntity.");
    };
    debug!(
        "Entity {} on {} requested assets {:?}.",
        entity, container_entity.root, asset_load.assets
    );

    let Ok(mut renderer_scene_context) = renderer_scene_contexts.get_mut(container_entity.root)
    else {
        debug_panic!("Root of AssetLoad does not contain RendererSceneContext.");
    };

    for file_path in &asset_load.assets {
        let ipfs_path = IpfsPath::new(IpfsType::new_content_file(
            renderer_scene_context.hash.to_owned(),
            file_path.to_owned(),
        ));
        let handle: Handle<LoadedUntypedAsset> =
            asset_server.load_untyped(PathBuf::from(&ipfs_path));

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
        debug_panic!("AssetLoad must be available to its observers.");
    };
    let Some(container_entity) = maybe_container_entity else {
        debug_panic!("AssetLoad entity did not have ContainerEntity.");
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
            debug_panic!("Could not get the AssetLoad of a PreloadedAsset.");
        };
        let Ok(mut renderer_scene_context) = renderer_scene_contexts.get_mut(container_entity.root)
        else {
            debug_panic!("Root of AssetLoad does not contain RendererSceneContext.");
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
                commands.entity(entity).despawn();
            }
            Some(RecursiveDependencyLoadState::Failed(err)) => {
                let loading_state = match err.as_ref() {
                    AssetLoadError::MissingAssetLoader { .. } => {
                        // Assume that assets with missing AssetLoaders are downloaded
                        // successfully, this is important for assets like videos
                        // as they are fed into ffmpeg instead of being added to the
                        // AssetServer
                        LoadingState::Finished
                    }
                    AssetLoadError::AssetReaderError(AssetReaderError::NotFound(_)) => {
                        LoadingState::NotFound
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
                commands.entity(entity).despawn();
            }
            None => {
                debug_panic!("Preload asset handle not found in asset server.");
            }
        }
    }
}
