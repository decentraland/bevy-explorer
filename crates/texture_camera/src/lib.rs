use std::collections::VecDeque;

use bevy::{
    core_pipeline::{
        bloom::BloomSettings,
        prepass::{DepthPrepass, NormalPrepass},
        tonemapping::{DebandDither, Tonemapping},
        Skybox,
    },
    pbr::ShadowFilteringMethod,
    prelude::*,
    render::{
        render_asset::RenderAssetUsages,
        render_resource::{Extent3d, TextureFormat, TextureUsages},
        texture::BevyDefault,
        view::{ColorGrading, ColorGradingGlobal, ColorGradingSection, Layer, RenderLayers},
    }, utils::hashbrown::HashMap,
};
use common::{
    dynamics::PLAYER_COLLIDER_RADIUS,
    sets::SceneSets,
    structs::{Cubemap, PrimaryUser, GROUND_RENDERLAYER, PRIMARY_AVATAR_LIGHT_LAYER},
};
use dcl_component::{
    proto_components::sdk::components::{PbCameraLayers, PbTextureCamera},
    SceneComponentId,
};
use scene_runner::{
    update_world::{material::VideoTextureOutput, AddCrdtInterfaceExt},
    ContainerEntity, ContainingScene,
};

pub struct TextureCameraPlugin;

impl Plugin for TextureCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbTextureCamera, TextureCamera>(
            SceneComponentId::TEXTURE_CAMERA,
            dcl::interface::ComponentPosition::EntityOnly,
        );
        app.add_crdt_lww_component::<PbCameraLayers, CameraLayers>(
            SceneComponentId::CAMERA_LAYERS,
            dcl::interface::ComponentPosition::EntityOnly,
        );

        app.add_systems(
            Update,
            (update_camera_layers, update_texture_cameras).in_set(SceneSets::PostLoop),
        );
    }
}

#[derive(Component)]
pub struct TextureCamera(pub PbTextureCamera);

impl From<PbTextureCamera> for TextureCamera {
    fn from(value: PbTextureCamera) -> Self {
        Self(value)
    }
}

#[derive(Component)]
pub struct TextureCamEntity(Entity);

#[allow(clippy::too_many_arguments)]
pub fn update_texture_cameras(
    mut commands: Commands,
    q: Query<(
        Entity,
        Ref<TextureCamera>,
        &ContainerEntity,
        Option<&TextureCamEntity>,
    )>,
    removed: Query<(Entity, &TextureCamEntity), Without<TextureCamera>>,
    mut images: ResMut<Assets<Image>>,
    mut cameras: Query<&mut Camera>,
    containing_scene: ContainingScene,
    player: Query<Entity, With<PrimaryUser>>,
    cubemap: Res<Cubemap>,
) {
    let active_scenes = player
        .get_single()
        .map(|p| containing_scene.get_area(p, PLAYER_COLLIDER_RADIUS))
        .unwrap_or_default();

    // remove cameras when TextureCam is removed
    for (ent, removed) in &removed {
        if let Some(commands) = commands.get_entity(removed.0) {
            commands.despawn_recursive();
        }
        commands.entity(ent).remove::<TextureCamEntity>();
    }

    for (ent, texture_cam, container, existing) in q.iter() {
        if texture_cam.is_changed() {
            // remove previous camera if modified
            if let Some(prev) = existing {
                if let Some(commands) = commands.get_entity(prev.0) {
                    commands.despawn_recursive();
                }
            }

            let mut image = Image::new_fill(
                Extent3d {
                    width: texture_cam.0.width.unwrap_or(256).clamp(16, 2048),
                    height: texture_cam.0.height.unwrap_or(256).clamp(16, 2048),
                    depth_or_array_layers: 1,
                },
                bevy::render::render_resource::TextureDimension::D2,
                &[255, 0, 255, 255],
                TextureFormat::bevy_default(),
                RenderAssetUsages::all(), // RENDER_WORLD alone doesn't work..?
            );

            image.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT;
            let image = images.add(image);

            let render_layers = match texture_cam.0.layer {
                None | Some(0) => {
                    RenderLayers::default().union(&GROUND_RENDERLAYER).union(&PRIMARY_AVATAR_LIGHT_LAYER)
                }
                Some(nonzero) => RenderLayers::layer(camera_to_render_layer(nonzero))
            };

            let camera_id = commands
                .spawn((
                    Camera3dBundle {
                        camera: Camera {
                            hdr: true,
                            order: isize::MIN + container.container_id.id as isize,
                            target: bevy::render::camera::RenderTarget::Image(image.clone()),
                            is_active: true,
                            ..Default::default()
                        },
                        tonemapping: Tonemapping::TonyMcMapface,
                        deband_dither: DebandDither::Enabled,
                        color_grading: ColorGrading {
                            // exposure: -0.5,
                            // gamma: 1.5,
                            // pre_saturation: 1.0,
                            // post_saturation: 1.0,
                            global: ColorGradingGlobal {
                                exposure: -0.5,
                                ..default()
                            },
                            shadows: ColorGradingSection {
                                gamma: 0.75,
                                ..Default::default()
                            },
                            midtones: ColorGradingSection {
                                gamma: 0.75,
                                ..Default::default()
                            },
                            highlights: ColorGradingSection {
                                gamma: 0.75,
                                ..Default::default()
                            },
                        },
                        projection: PerspectiveProjection {
                            // projection: OrthographicProjection {
                            far: 100000.0,
                            ..Default::default()
                        }
                        .into(),
                        ..Default::default()
                    },
                    BloomSettings {
                        intensity: 0.15,
                        ..BloomSettings::OLD_SCHOOL
                    },
                    ShadowFilteringMethod::Gaussian,
                    DepthPrepass,
                    NormalPrepass,
                    render_layers,
                    Skybox {
                        image: cubemap.image_handle.clone(),
                        brightness: 1000.0,
                    },
                ))
                .id();

            commands
                .entity(ent)
                .push_children(&[camera_id])
                .insert((TextureCamEntity(camera_id), VideoTextureOutput(image)));
        } else {
            // set active for current scenes only
            // TODO: limit / cycle
            let Some(existing) = existing else {
                warn!("missing TextureCameraEntity");
                continue;
            };

            let Ok(mut camera) = cameras.get_mut(existing.0) else {
                warn!("missing camera entity for TextureCamera");
                continue;
            };

            camera.is_active = active_scenes.contains(&container.root);
        }
    }
}

#[derive(Component)]
pub struct CameraLayers(pub Vec<u32>);

impl From<PbCameraLayers> for CameraLayers {
    fn from(value: PbCameraLayers) -> Self {
        Self(value.layers)
    }
}

fn camera_to_render_layer(camera_layer: u32) -> Layer {
    (match camera_layer {
        0 => 0,
        nonzero => nonzero + 5
    }) as Layer
}

fn camera_to_render_layers<'a>(camera_layers: impl Iterator<Item=&'a u32>) -> RenderLayers {
    camera_layers.fold(RenderLayers::none(), |result, camera_layer| {
        result.with(camera_to_render_layer(*camera_layer))
    })
}


pub fn update_camera_layers(
    mut commands: Commands,
    mut removed: RemovedComponents<CameraLayers>,
    maybe_changed: Query<(Entity, Option<&CameraLayers>, &Parent), Or<(Changed<CameraLayers>, Changed<Parent>)>>,
    removed_data: Query<(Option<&CameraLayers>, &Parent)>,
    children: Query<&Children>,
    render_layers: Query<&RenderLayers>,
) {
    let mut to_check = VecDeque::default();

    // gather items that have changed
    for (entity, maybe_layers, parent) in &maybe_changed {
        let target_render_layers = if let Some(camera_layers) = maybe_layers {
            camera_to_render_layers(camera_layers.0.iter())
        } else {
            render_layers.get(parent.get()).cloned().unwrap_or_else(|_| RenderLayers::default())
        };

        to_check.push_back((entity, target_render_layers));
    }

    // or had explicit layers removed
    for removed_entity in removed.read() {
        // (and still exist)
        let Ok((maybe_layers, parent)) = removed_data.get(removed_entity) else {
            continue;
        };

        let target_render_layers = if let Some(camera_layers) = maybe_layers {
            camera_to_render_layers(camera_layers.0.iter())
        } else {
            render_layers.get(parent.get()).cloned().unwrap_or_else(|_| RenderLayers::default())
        };

        to_check.push_back((removed_entity, target_render_layers));
    }

    let mut updated = HashMap::<Entity, RenderLayers>::default();

    // for entities that may need updating
    while let Some((entity, target_layers)) = to_check.pop_front() {
        // if we already updated to this value stop here
        if let Some(update) = updated.get(&entity) {
            if update == &target_layers {
                continue;
            }
        } 
        // if we didn't already update and the existing data matches the requirement then stop here
        else if render_layers.get(entity).unwrap_or(&RenderLayers::default()) == &target_layers {
            continue;
        }

        // update
        commands.entity(entity).insert(target_layers.clone());
        // check children
        for child in children.get(entity).map(IntoIterator::into_iter).unwrap_or_default() {
            to_check.push_back((*child, target_layers.clone()));
        }
        updated.insert(entity, target_layers);
    }
}
