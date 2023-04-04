use bevy::{gltf::Gltf, prelude::*, scene::InstanceId};

use crate::{
    dcl::interface::ComponentPosition,
    dcl_component::{proto_components::sdk::components::PbGltfContainer, SceneComponentId},
    ipfs::{IpfsLoaderExt, SceneDefinition},
    scene_runner::{SceneEntity, SceneSets},
};

use super::AddCrdtInterfaceExt;

pub struct GltfDefinitionPlugin;

#[derive(Component, Debug)]
pub struct GltfDefinition(PbGltfContainer);

impl From<PbGltfContainer> for GltfDefinition {
    fn from(value: PbGltfContainer) -> Self {
        Self(value)
    }
}

impl Plugin for GltfDefinitionPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbGltfContainer, GltfDefinition>(
            SceneComponentId::GLTF_CONTAINER,
            ComponentPosition::EntityOnly,
        );

        app.add_system(update_gltf.in_set(SceneSets::PostLoop));
    }
}

#[derive(Component)]
struct GltfProcessed(Option<InstanceId>);
#[derive(Component)]
struct GltfPruned;

#[allow(clippy::too_many_arguments)]
fn update_gltf(
    mut commands: Commands,
    new_gltfs: Query<(Entity, &SceneEntity, &GltfDefinition), Changed<GltfDefinition>>,
    unprocessed_gltfs: Query<
        (Entity, &SceneEntity, &Handle<Gltf>, &GltfDefinition),
        Without<GltfProcessed>,
    >,
    ready_gltfs: Query<(Entity, &GltfProcessed), Without<GltfPruned>>,
    mut named_entities: Query<(Option<&Name>, &mut Transform, &Parent)>,
    scene_def_handles: Query<&Handle<SceneDefinition>>,
    scene_defs: Res<Assets<SceneDefinition>>,
    asset_server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    mut scene_spawner: ResMut<SceneSpawner>,
) {
    // TODO: clean up old gltf data

    for (ent, scene_ent, gltf) in new_gltfs.iter() {
        info!("{} has {}", scene_ent.id, gltf.0.src);

        let Ok(h_scene_def) = scene_def_handles.get(scene_ent.root) else {
            warn!("no scene definition found, can't process file request");
            continue;
        };

        let Some(scene_def) = scene_defs.get(h_scene_def) else {
            warn!("scene definition not loaded, can't process file request");
            continue;
        };

        let h_gltf = asset_server.load_scene_file::<Gltf>(gltf.0.src.as_str(), &scene_def.id);

        commands
            .entity(ent)
            .insert(h_gltf)
            .remove::<GltfProcessed>();
    }

    for (ent, _scene_ent, h_gltf, gtlf_def) in unprocessed_gltfs.iter() {
        match asset_server.get_load_state(h_gltf) {
            bevy::asset::LoadState::Loaded => (),
            bevy::asset::LoadState::Failed => {
                warn!("failed to process gltf");
                commands.entity(ent).insert(GltfProcessed(None));
                continue;
            }
            _ => continue,
        }

        let gltf = gltfs.get(h_gltf).unwrap();
        println!("gltf `{}`: {:#?}", gtlf_def.0.src, gltf);

        let gltf_scene_handle = gltf.default_scene.as_ref().unwrap();
        let instance_id = scene_spawner.spawn_as_child(gltf_scene_handle.clone_weak(), ent);
        commands
            .entity(ent)
            .insert(GltfProcessed(Some(instance_id)));
    }

    for (ent, processed) in ready_gltfs.iter() {
        if processed.0.is_none() {
            commands.entity(ent).insert(GltfPruned);
            continue;
        }
        let instance = processed.0.as_ref().unwrap();
        if scene_spawner.instance_is_ready(*instance) {
            for spawned_ent in scene_spawner.iter_instance_entities(*instance) {
                if let Ok((maybe_name, mut transform, parent)) = named_entities.get_mut(spawned_ent)
                {
                    if maybe_name
                        .map(|name| name.as_str().ends_with("_collider"))
                        .unwrap_or(false)
                    {
                        // TODO interpret as collider
                        commands.entity(spawned_ent).despawn_recursive();
                    } else {
                        info!("keeping {:?}", maybe_name);
                        // why?!
                        if parent.get() == ent {
                            transform.rotate_around(
                                Vec3::ZERO,
                                Quat::from_rotation_y(std::f32::consts::PI),
                            );
                        }
                    }
                }
            }

            commands.entity(ent).insert(GltfPruned);
        }
    }
}
