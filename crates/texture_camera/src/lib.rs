use std::{collections::VecDeque, f32::consts::FRAC_PI_4};

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
        view::{ColorGrading, ColorGradingGlobal, ColorGradingSection, RenderLayers},
    },
    utils::hashbrown::HashMap,
};
use bevy_atmosphere::plugin::AtmosphereCamera;
use common::{
    dynamics::PLAYER_COLLIDER_RADIUS,
    sets::SceneSets,
    structs::{AppConfig, Cubemap, PrimaryUser, GROUND_RENDERLAYER, PRIMARY_AVATAR_LIGHT_LAYER},
    util::{camera_to_render_layer, camera_to_render_layers},
};
use dcl_component::{
    proto_components::sdk::components::{PbCameraLayers, PbTextureCamera},
    SceneComponentId,
};
use scene_runner::{
    update_world::{material::VideoTextureOutput, AddCrdtInterfaceExt},
    ContainerEntity, ContainingScene,
};
use system_bridge::settings::NewCameraEvent;
use visuals::SceneGlobalLight;

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
    mut new_cam_events: EventWriter<NewCameraEvent>,
    global_light: Res<SceneGlobalLight>,
    config: Res<AppConfig>,
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
                None | Some(0) => RenderLayers::default()
                    .union(&GROUND_RENDERLAYER)
                    .union(&PRIMARY_AVATAR_LIGHT_LAYER),
                Some(nonzero) => RenderLayers::layer(camera_to_render_layer(nonzero)),
            };
            println!("create with layers {render_layers:?}");

            let far = texture_cam.0.far_plane.unwrap_or(100_000.0);
            let projection: Projection = match &texture_cam.0.mode {
                None => {
                    PerspectiveProjection {
                        far,
                        ..Default::default()
                    }
                    .into()
                }
                Some(dcl_component::proto_components::sdk::components::pb_texture_camera::Mode::Perspective(p)) => {
                    PerspectiveProjection {
                        fov: p.field_of_view.unwrap_or(FRAC_PI_4),
                        far,
                        ..Default::default()
                    }.into()
                }
                Some(dcl_component::proto_components::sdk::components::pb_texture_camera::Mode::Orthographic(o)) => {
                    OrthographicProjection {
                        far,
                        scaling_mode: bevy::render::camera::ScalingMode::FixedVertical(o.vertical_range.unwrap_or(4.0)),
                        ..Default::default()
                    }.into()
                }
            };

            let mut camera = commands.spawn((
                Camera3dBundle {
                    camera: Camera {
                        hdr: true,
                        order: isize::MIN + container.container_id.id as isize,
                        target: bevy::render::camera::RenderTarget::Image(image.clone()),
                        clear_color: ClearColorConfig::Custom(
                            texture_cam
                                .0
                                .clear_color
                                .map(Color::from)
                                .unwrap_or(Color::BLACK),
                        ),
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
                    projection,
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
            ));

            if !texture_cam.0.disable_fog() {
                camera.insert(FogSettings::default());
            }

            if !texture_cam.0.disable_skybox()
                && !matches!(texture_cam.0.mode, Some(dcl_component::proto_components::sdk::components::pb_texture_camera::Mode::Orthographic(_)))
            {
                camera.insert((
                    Skybox {
                        image: cubemap.image_handle.clone(),
                        brightness: 1000.0,
                    },
                    AtmosphereCamera::default()
                ));
            }

            if texture_cam.0.ambient_brightness_override.is_some()
                || texture_cam.0.ambient_color_override.is_some()
            {
                camera.insert(AmbientLight {
                    color: texture_cam
                        .0
                        .ambient_color_override
                        .map(Color::from)
                        .unwrap_or(global_light.ambient_color),
                    brightness: texture_cam
                        .0
                        .ambient_brightness_override
                        .unwrap_or(global_light.ambient_brightness)
                        * config.graphics.ambient_brightness as f32
                        * 20.0,
                });
            }

            let camera_id = camera.id();

            commands
                .entity(ent)
                .push_children(&[camera_id])
                .insert((TextureCamEntity(camera_id), VideoTextureOutput(image)));

            new_cam_events.send(NewCameraEvent(camera_id));
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

#[allow(clippy::type_complexity)]
pub fn update_camera_layers(
    mut commands: Commands,
    mut removed: RemovedComponents<CameraLayers>,
    maybe_changed: Query<
        (Entity, Option<&CameraLayers>, &Parent),
        (
            Without<Camera>,
            Or<(Changed<CameraLayers>, Changed<Parent>)>,
        ),
    >,
    removed_data: Query<(Option<&CameraLayers>, &Parent)>,
    children: Query<&Children, Without<Camera>>,
    render_layers: Query<&RenderLayers>,
) {
    let mut to_check = VecDeque::default();

    // gather items that have changed
    for (entity, maybe_layers, parent) in &maybe_changed {
        let target_render_layers = if let Some(camera_layers) = maybe_layers {
            camera_to_render_layers(camera_layers.0.iter())
        } else {
            render_layers
                .get(parent.get())
                .cloned()
                .unwrap_or_else(|_| RenderLayers::default())
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
            render_layers
                .get(parent.get())
                .cloned()
                .unwrap_or_else(|_| RenderLayers::default())
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
        else if render_layers
            .get(entity)
            .unwrap_or(&RenderLayers::default())
            == &target_layers
        {
            continue;
        }

        // update
        commands.entity(entity).insert(target_layers.clone());
        // check children
        for child in children
            .get(entity)
            .map(IntoIterator::into_iter)
            .unwrap_or_default()
        {
            to_check.push_back((*child, target_layers.clone()));
        }
        updated.insert(entity, target_layers);
    }
}
