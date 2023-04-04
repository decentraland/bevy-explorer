use bevy::{prelude::*, utils::HashMap};

use crate::{
    dcl::{interface::CrdtComponentInterfaces, spawn_scene},
    dcl_component::SceneEntityId,
    ipfs::{IpfsIo, IpfsLoaderExt, SceneDefinition, SceneIpfsLocation, SceneJsFile, SceneMeta},
    scene_runner::{
        renderer_context::RendererSceneContext, DeletedSceneEntities, SceneEntity,
        SceneThreadHandle,
    },
};

use super::{update_world::CrdtExtractors, LoadSceneEvent, SceneUpdates};

#[derive(Component)]
pub enum SceneLoading {
    SceneEntity,
    SceneMeta,
    Javascript,
}

pub(crate) fn load_scene_entity(
    mut commands: Commands,
    mut load_scene_events: EventReader<LoadSceneEvent>,
    asset_server: Res<AssetServer>,
) {
    for event in load_scene_events.iter() {
        match &event.location {
            SceneIpfsLocation::Pointer(x, y) => {
                commands.spawn((
                    SceneLoading::SceneEntity,
                    asset_server.load_scene_pointer(*x, *y),
                ));
            }
            SceneIpfsLocation::Hash(path) => {
                commands.spawn((
                    SceneLoading::SceneEntity,
                    asset_server.load::<SceneDefinition, _>(format!("{path}.scene_entity")),
                ));
            }
            SceneIpfsLocation::Js(path) => {
                commands.spawn((
                    SceneLoading::Javascript,
                    asset_server.load::<SceneJsFile, _>(format!("{path}.js")),
                ));
            }
        };
    }
}

pub(crate) fn load_scene_json(
    mut commands: Commands,
    mut loading_scenes: Query<(Entity, &mut SceneLoading, &Handle<SceneDefinition>)>,
    scene_definitions: Res<Assets<SceneDefinition>>,
    asset_server: Res<AssetServer>,
) {
    for (entity, mut state, h_scene) in loading_scenes
        .iter_mut()
        .filter(|(_, state, _)| matches!(**state, SceneLoading::SceneEntity))
    {
        let mut fail = |msg: &str| {
            warn!("{entity:?} failed to initialize scene: {msg}");
            commands.entity(entity).despawn_recursive();
        };

        match asset_server.get_load_state(h_scene) {
            bevy::asset::LoadState::Loaded => (),
            bevy::asset::LoadState::Failed => {
                fail("Scene entity could not be loaded");
                continue;
            }
            _ => continue,
        }
        let Some(definition) = scene_definitions.get(h_scene) else {
            fail("Scene entity did not resolve to a valid asset");
            continue;
        };

        let ipfs_io = asset_server.asset_io().downcast_ref::<IpfsIo>().unwrap();
        ipfs_io.add_collection(definition.id.clone(), definition.content.clone());

        let h_meta = asset_server.load_scene_file::<SceneMeta>("scene.json", &definition.id);

        commands.entity(entity).insert(h_meta);
        *state = SceneLoading::SceneMeta;
    }
}

pub(crate) fn load_scene_javascript(
    mut commands: Commands,
    mut loading_scenes: Query<(
        Entity,
        &mut SceneLoading,
        &Handle<SceneDefinition>,
        &Handle<SceneMeta>,
    )>,
    scene_definitions: Res<Assets<SceneDefinition>>,
    scene_metas: Res<Assets<SceneMeta>>,
    asset_server: Res<AssetServer>,
) {
    for (entity, mut state, h_scene, h_meta) in loading_scenes
        .iter_mut()
        .filter(|(_, state, _, _)| matches!(**state, SceneLoading::SceneMeta))
    {
        let mut fail = |msg: &str| {
            warn!("{entity:?} failed to initialize scene: {msg}");
            commands.entity(entity).despawn_recursive();
        };

        match asset_server.get_load_state(h_meta) {
            bevy::asset::LoadState::Loaded => (),
            bevy::asset::LoadState::Failed => {
                fail("scene.json could not be loaded");
                continue;
            }
            _ => continue,
        }
        let definition = scene_definitions.get(h_scene).unwrap();
        let Some(meta) = scene_metas.get(h_meta) else {
            fail("scene.json did not resolve to expected format");
            continue;
        };
        let h_code = asset_server.load_scene_file::<SceneJsFile>(&meta.main, &definition.id);

        commands.entity(entity).insert(h_code);
        *state = SceneLoading::Javascript;
    }
}

pub(crate) fn initialize_scene(
    mut commands: Commands,
    mut scene_updates: ResMut<SceneUpdates>,
    crdt_component_interfaces: Res<CrdtExtractors>,
    loading_scenes: Query<(Entity, &SceneLoading, &Handle<SceneJsFile>)>,
    scene_js_files: Res<Assets<SceneJsFile>>,
    asset_server: Res<AssetServer>,
) {
    for (root, _, h_code) in loading_scenes
        .iter()
        .filter(|(_, state, ..)| matches!(state, SceneLoading::Javascript))
    {
        debug!("checking for js");
        let mut fail = |msg: &str| {
            warn!("{root:?} failed to initialize scene: {msg}");
            commands.entity(root).despawn_recursive();
        };

        match asset_server.get_load_state(h_code) {
            bevy::asset::LoadState::Loaded => (),
            bevy::asset::LoadState::Failed => {
                fail("main js could not be loaded");
                continue;
            }
            _ => continue,
        }

        let Some(js_file) = scene_js_files.get(h_code) else {
            fail("main js did not resolve to expected format");
            continue;
        };

        info!("{root:?}: starting scene");

        // create the scene root entity
        // todo set world position
        commands.entity(root).remove::<SceneLoading>().insert((
            SpatialBundle {
                // todo set world position
                ..Default::default()
            },
            DeletedSceneEntities::default(),
        ));

        let thread_sx = scene_updates.sender.clone();

        let crdt_component_interfaces = CrdtComponentInterfaces(HashMap::from_iter(
            crdt_component_interfaces
                .0
                .iter()
                .map(|(id, interface)| (*id, interface.crdt_type())),
        ));

        let (scene_id, main_sx) =
            spawn_scene(js_file.clone(), crdt_component_interfaces, thread_sx);

        let renderer_context = RendererSceneContext::new(scene_id, root, 1.0);

        scene_updates.scene_ids.insert(scene_id, root);

        commands.entity(root).insert((
            renderer_context,
            SceneEntity {
                root,
                scene_id,
                id: SceneEntityId::ROOT,
            },
            SceneThreadHandle { sender: main_sx },
        ));
    }
}
