use std::f32::consts::FRAC_PI_4;

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
    utils::{hashbrown::HashMap, HashSet},
};
use bevy_atmosphere::plugin::AtmosphereCamera;
use common::{
    dynamics::PLAYER_COLLIDER_RADIUS,
    sets::SceneSets,
    structs::{AppConfig, Cubemap, PrimaryUser, GROUND_RENDERLAYER, PRIMARY_AVATAR_LIGHT_LAYER},
    util::camera_to_render_layers,
};
use comms::global_crdt::ForeignPlayer;
use dcl_component::{
    proto_components::sdk::components::{PbCameraLayer, PbCameraLayers, PbTextureCamera},
    SceneComponentId,
};
use propagate::{HierarchyPropagatePlugin, Propagate, PropagateStop};
use scene_runner::{
    renderer_context::RendererSceneContext,
    update_world::{
        lights::update_directional_light, material::VideoTextureOutput, AddCrdtInterfaceExt,
    },
    ContainerEntity, ContainingScene,
};
use system_bridge::settings::NewCameraEvent;
use visuals::SceneGlobalLight;

pub struct TextureCameraPlugin;

impl Plugin for TextureCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TextureLayersCache>()
            .init_resource::<SceneLayerProperties>();
        app.add_systems(PostUpdate, TextureLayersCache::cleanup);

        app.add_plugins(HierarchyPropagatePlugin::<RenderLayers, ()>::default());

        app.add_crdt_lww_component::<PbTextureCamera, TextureCamera>(
            SceneComponentId::TEXTURE_CAMERA,
            dcl::interface::ComponentPosition::EntityOnly,
        );
        app.add_crdt_lww_component::<PbCameraLayers, CameraLayers>(
            SceneComponentId::CAMERA_LAYERS,
            dcl::interface::ComponentPosition::Any,
        );
        app.add_crdt_lww_component::<PbCameraLayer, CameraLayer>(
            SceneComponentId::CAMERA_LAYER,
            dcl::interface::ComponentPosition::Any,
        );

        app.add_systems(
            Update,
            (
                update_layer_properties,
                update_camera_layers,
                update_texture_cameras,
                update_avatar_layers,
                update_directional_light_layers.after(update_directional_light),
            )
                .in_set(SceneSets::PostLoop),
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
pub struct CameraLayer(pub PbCameraLayer);

impl From<PbCameraLayer> for CameraLayer {
    fn from(value: PbCameraLayer) -> Self {
        Self(value)
    }
}

#[derive(Resource, Default)]
pub struct SceneLayerProperties {
    layers: HashMap<u32, (Entity, PbCameraLayer)>,
    ent_to_layer: HashMap<Entity, u32>,
}

fn update_layer_properties(
    q: Query<(Entity, &CameraLayer, &ContainerEntity), Changed<CameraLayer>>,
    mut removed: RemovedComponents<CameraLayer>,
    mut props: ResMut<SceneLayerProperties>,
    mut cache: ResMut<TextureLayersCache>,
) {
    let mut changed = HashSet::default();

    for removed in removed.read() {
        if let Some(layer) = props.ent_to_layer.remove(&removed) {
            if props.layers.get(&layer).is_some_and(|(e, _)| e == &removed) {
                props.layers.remove(&layer);
                cache.changed_layers.insert(layer);
            }
        }
    }

    for (ent, layer, container) in q.iter() {
        if let Some(layer) = props.ent_to_layer.remove(&ent) {
            if props.layers.get(&layer).is_some_and(|(e, _)| e == &ent) {
                props.layers.remove(&layer);
                changed.insert(layer);
            }
        }

        if layer.0.layer == 0 {
            warn!("can't update layer 0");
            continue;
        }

        let render_layer = cache.get_layer(container.root, layer.0.layer);
        props.layers.insert(render_layer, (ent, layer.0.clone()));
        cache.changed_layers.insert(render_layer);
        debug!("changed layer {:?} -> {:?}", render_layer, &layer.0);
    }
}

#[allow(clippy::type_complexity)]
fn update_avatar_layers(
    mut qs: ParamSet<(
        Query<&mut Propagate<RenderLayers>, Or<(Added<ForeignPlayer>, Added<PrimaryUser>)>>,
        Query<&mut Propagate<RenderLayers>, Or<(With<ForeignPlayer>, With<PrimaryUser>)>>,
    )>,
    cache: Res<TextureLayersCache>,
    props: Res<SceneLayerProperties>,
) {
    let mut new = qs.p0();

    if !new.is_empty() {
        let all_layers = props
            .layers
            .iter()
            .filter(|(_, (_, layer))| layer.show_avatars())
            .map(|(layer_ix, _)| *layer_ix)
            .collect::<Vec<_>>();

        for mut render_layer in new.iter_mut() {
            for layer in &all_layers {
                render_layer.0 = render_layer.0.clone().with(*layer as usize);
            }
        }
    }

    let mut all = qs.p1();
    for changed_layer in cache.changed_layers.iter() {
        let show = props
            .layers
            .get(changed_layer)
            .map(|(_, l)| l.show_avatars())
            .unwrap_or(false);
        for mut render_layer in all.iter_mut() {
            if show {
                render_layer.0 = render_layer.0.clone().with(*changed_layer as usize);
            } else {
                render_layer.0 = render_layer.0.clone().without(*changed_layer as usize);
            }
            debug!("avatar layer -> {:?}", render_layer.0);
        }
    }
}

fn update_directional_light_layers(
    props: Res<SceneLayerProperties>,
    mut global_light: ResMut<SceneGlobalLight>,
) {
    for (layer, _) in props
        .layers
        .iter()
        .filter(|(_, (_, props))| props.directional_light())
    {
        global_light.layers = global_light.layers.clone().with(*layer as usize);
    }
}

#[derive(Component)]
pub struct TextureCamEntity(Entity);

#[allow(clippy::too_many_arguments)]
fn update_texture_cameras(
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
    layers: Res<SceneLayerProperties>,
    mut layer_cache: ResMut<TextureLayersCache>,
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

    // (re)create new/modified cams
    for (ent, texture_cam, container, existing) in q.iter() {
        let layer_ix = layer_cache.get_layer(container.root, texture_cam.0.layer.unwrap_or(0));
        if texture_cam.is_changed() || layer_cache.changed_layers.contains(&layer_ix) {
            // remove previous camera if modified
            if let Some(prev) = existing {
                if let Some(commands) = commands.get_entity(prev.0) {
                    commands.despawn_recursive();
                }
            }

            let maybe_layer = layers.layers.get(&layer_ix).map(|(_, layer)| layer);

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

            let render_layers = match layer_ix {
                0 => RenderLayers::default()
                    .union(&GROUND_RENDERLAYER)
                    .union(&PRIMARY_AVATAR_LIGHT_LAYER),
                _ => RenderLayers::layer(layer_ix as usize),
            };
            debug!("create with layers {render_layers:?}");

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
                    // not sure why but seems like we need to invert the look direction
                    // transform: Transform::from_rotation(Quat::inverse(Quat::IDENTITY)),
                    ..Default::default()
                },
                BloomSettings {
                    intensity: 0.15,
                    ..BloomSettings::OLD_SCHOOL
                },
                ShadowFilteringMethod::Gaussian,
                DepthPrepass,
                NormalPrepass,
                render_layers.clone(),
                PropagateStop::<RenderLayers>::default(),
            ));

            if maybe_layer.is_some_and(|l| l.show_fog()) {
                camera.insert(FogSettings::default());
            }

            if maybe_layer.is_some_and(|l| l.show_skybox())
                && !matches!(texture_cam.0.mode, Some(dcl_component::proto_components::sdk::components::pb_texture_camera::Mode::Orthographic(_)))
            {
                camera.insert((
                    Skybox {
                        image: cubemap.image_handle.clone(),
                        brightness: 1000.0,
                    },
                    AtmosphereCamera {
                        render_layers: Some(render_layers.clone()),
                    }
                ));
            }

            if maybe_layer.is_some_and(|l| {
                l.ambient_brightness_override.is_some() || l.ambient_color_override.is_some()
            }) {
                camera.insert(AmbientLight {
                    color: maybe_layer
                        .and_then(|l| l.ambient_color_override)
                        .map(Color::from)
                        .unwrap_or(global_light.ambient_color),
                    brightness: maybe_layer
                        .and_then(|l| l.ambient_brightness_override)
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

            let is_active = active_scenes.contains(&container.root);
            // debug!("[{}] active: {}", container.container_id, is_active);
            camera.is_active = is_active;
        }
    }
}

#[derive(Component)]
pub struct CameraLayers(pub Vec<u32>);

impl From<PbCameraLayers> for CameraLayers {
    fn from(value: PbCameraLayers) -> Self {
        if value.layers.is_empty() {
            Self(vec![0])
        } else {
            Self(value.layers)
        }
    }
}

#[allow(clippy::type_complexity)]
fn update_camera_layers(
    mut commands: Commands,
    changed: Query<(Entity, &CameraLayers, &ContainerEntity), Changed<CameraLayers>>,
    changed_root: Query<
        (Entity, &CameraLayers),
        (Changed<CameraLayers>, With<RendererSceneContext>),
    >,
    mut removed: RemovedComponents<CameraLayers>,
    mut store: ResMut<TextureLayersCache>,
) {
    for (entity, layers, container) in changed.iter() {
        let base = store.get_base(container.root);
        let layers = camera_to_render_layers(base, layers.0.iter());
        commands.entity(entity).try_insert(Propagate(layers));
    }
    for (root, layers) in changed_root.iter() {
        let base = store.get_base(root);
        let layers = camera_to_render_layers(base, layers.0.iter());
        commands.entity(root).try_insert(Propagate(layers));
    }

    for entity in removed.read() {
        if let Some(mut commands) = commands.get_entity(entity) {
            commands.remove::<Propagate<RenderLayers>>();
        }
    }
}

#[derive(Resource, Default)]
pub struct TextureLayersCache {
    pub free: HashSet<u32>,
    pub lookup: HashMap<Entity, u32>,
    pub max: u32,
    pub changed_layers: HashSet<u32>,
}

const LAYERS_PER_SCENE: u32 = 15;

impl TextureLayersCache {
    pub fn get_base(&mut self, scene: Entity) -> u32 {
        match self.lookup.get(&scene) {
            Some(existing) => existing * LAYERS_PER_SCENE,
            None => {
                let base = self
                    .free
                    .iter()
                    .next()
                    .copied()
                    .inspect(|base| {
                        self.free.remove(base);
                    })
                    .unwrap_or_else(|| {
                        self.max += 1;
                        self.max
                    });
                self.lookup.insert(scene, base);
                base * LAYERS_PER_SCENE
            }
        }
    }

    pub fn get_layer(&mut self, scene: Entity, layer: u32) -> u32 {
        if layer == 0 {
            return 0;
        } else if layer > LAYERS_PER_SCENE {
            warn!("layer too high");
        }
        self.get_base(scene) + (layer - 1).min(LAYERS_PER_SCENE)
    }

    pub fn cleanup(mut slf: ResMut<Self>, scenes: Query<Entity, With<RendererSceneContext>>) {
        let mut free = std::mem::take(&mut slf.free);
        slf.lookup.retain(|scene, base| {
            let live = scenes.get(*scene).is_ok();
            if !live {
                free.insert(*base);
            }
            live
        });
        slf.changed_layers.clear();
        slf.free = free;
    }
}
