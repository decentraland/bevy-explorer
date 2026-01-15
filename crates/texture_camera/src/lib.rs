use std::f32::consts::FRAC_PI_4;

use bevy::{
    app::{HierarchyPropagatePlugin, Propagate, PropagateSet, PropagateStop}, asset::RenderAssetTransferPriority, platform::collections::{HashMap, HashSet}, prelude::*, render::{
        render_asset::RenderAssetUsages,
        render_resource::{Extent3d, TextureFormat, TextureUsages},
        view::RenderLayers,
    }
};
use bevy_atmosphere::plugin::AtmosphereCamera;
use common::{
    dynamics::PLAYER_COLLIDER_RADIUS,
    sets::SceneSets,
    structs::{
        AppConfig, PrimaryUser, SceneGlobalLight, SceneLoadDistance, GROUND_RENDERLAYER,
        PRIMARY_AVATAR_LIGHT_LAYER,
    },
    util::{camera_to_render_layers, AudioReceiver, TryPushChildrenEx},
};
use comms::global_crdt::ForeignPlayer;
use dcl_component::{
    proto_components::{
        sdk::components::{PbCameraLayer, PbCameraLayers, PbTextureCamera},
        Color3DclToBevy, Color4DclToBevy,
    },
    SceneComponentId,
};
use platform::default_camera_components;
use scene_runner::{
    renderer_context::RendererSceneContext,
    update_world::{
        lights::update_directional_light, material::VideoTextureOutput, AddCrdtInterfaceExt,
    },
    ContainerEntity, ContainingScene,
};
use system_bridge::settings::NewCameraEvent;

pub struct TextureCameraPlugin;

impl Plugin for TextureCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TextureLayersCache>()
            .init_resource::<SceneLayerProperties>();
        app.add_systems(PostUpdate, TextureLayersCache::cleanup);

        app.add_plugins(HierarchyPropagatePlugin::<RenderLayers>::default());

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
                update_directional_light_layers
                    .after(update_directional_light)
                    .before(PropagateSet::<RenderLayers>::default()),
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
    let mut changed = HashSet::new();

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

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_texture_cameras(
    mut commands: Commands,
    q: Query<(
        Entity,
        Ref<TextureCamera>,
        &ContainerEntity,
        Option<&TextureCamEntity>,
        Option<&VideoTextureOutput>,
    )>,
    removed: Query<(Entity, &TextureCamEntity), Without<TextureCamera>>,
    mut images: ResMut<Assets<Image>>,
    mut cameras: Query<&mut Camera>,
    containing_scene: ContainingScene,
    player: Query<Entity, With<PrimaryUser>>,
    mut new_cam_events: EventWriter<NewCameraEvent>,
    global_light: Res<SceneGlobalLight>,
    config: Res<AppConfig>,
    layers: Res<SceneLayerProperties>,
    mut layer_cache: ResMut<TextureLayersCache>,
    scene_distance: Res<SceneLoadDistance>,
) {
    let active_scenes = player
        .single()
        .map(|p| containing_scene.get_area(p, PLAYER_COLLIDER_RADIUS))
        .unwrap_or_default();

    // remove cameras when TextureCam is removed
    for (ent, removed) in &removed {
        if let Ok(mut commands) = commands.get_entity(removed.0) {
            commands.despawn();
        }
        commands.entity(ent).remove::<TextureCamEntity>();
    }

    // (re)create new/modified cams
    for (ent, texture_cam, container, maybe_existing_camera, maybe_existing_image) in q.iter() {
        let layer_ix = layer_cache.get_layer(container.root, texture_cam.0.layer.unwrap_or(0));
        if texture_cam.is_changed() || layer_cache.changed_layers.contains(&layer_ix) {
            // remove previous camera if modified
            if let Some(prev) = maybe_existing_camera {
                if let Ok(mut commands) = commands.get_entity(prev.0) {
                    commands.despawn();
                }
            }

            let maybe_layer = layers.layers.get(&layer_ix).map(|(_, layer)| layer);

            let image_size = Extent3d {
                width: texture_cam.0.width.unwrap_or(256).clamp(16, 2048),
                height: texture_cam.0.height.unwrap_or(256).clamp(16, 2048),
                depth_or_array_layers: 1,
            };

            let image = match maybe_existing_image {
                Some(existing) => {
                    let prev = images.get_mut(existing.0.id()).unwrap();
                    if prev.texture_descriptor.size != image_size {
                        prev.texture_descriptor.size = image_size;
                    }
                    existing.0.clone()
                }
                None => {
                    let mut image = Image::new_fill(
                        image_size,
                        bevy::render::render_resource::TextureDimension::D2,
                        &[255, 0, 255, 255],
                        TextureFormat::bevy_default(),
                        RenderAssetUsages::all(), // RENDER_WORLD alone doesn't work..?
                    );
                    image.data = None;
                    image.transfer_priority = RenderAssetTransferPriority::Immediate;
                    image.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT;
                    images.add(image)
                }
            };

            let render_layers = match layer_ix {
                0 => RenderLayers::default()
                    .union(&GROUND_RENDERLAYER)
                    .union(&PRIMARY_AVATAR_LIGHT_LAYER),
                _ => RenderLayers::layer(layer_ix as usize),
            };
            debug!("create with layers {render_layers:?}");

            let far = texture_cam.0.far_plane.unwrap_or(240.0);
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
                        scaling_mode: bevy::render::camera::ScalingMode::FixedVertical{ viewport_height: o.vertical_range.unwrap_or(4.0) },
                        ..OrthographicProjection::default_3d()
                    }.into()
                }
            };

            let mut camera = commands.spawn((
                Camera3d::default(),
                Camera {
                    hdr: true,
                    order: isize::MIN + container.container_id.id as isize,
                    target: bevy::render::camera::RenderTarget::Image(image.clone().into()),
                    clear_color: ClearColorConfig::Custom(
                        texture_cam
                            .0
                            .clear_color
                            .map(Color4DclToBevy::convert_srgba)
                            .unwrap_or(Color::BLACK),
                    ),
                    is_active: true,
                    ..Default::default()
                },
                default_camera_components(),
                projection.clone(),
                render_layers.clone(),
                PropagateStop::<RenderLayers>::default(),
            ));

            if maybe_layer.is_some_and(|l| l.show_fog()) {
                let distance = texture_cam.0.far_plane.unwrap_or(
                    (scene_distance.load + scene_distance.unload)
                        .max(scene_distance.load_imposter * 0.333),
                );

                camera.insert(DistanceFog {
                    color: Color::srgb(0.3, 0.2, 0.1),
                    directional_light_color: Color::srgb(1.0, 1.0, 0.7),
                    directional_light_exponent: 10.0,
                    falloff: FogFalloff::from_visibility_squared(distance * 2.0),
                });
            }

            if maybe_layer.is_some_and(|l| l.show_skybox())
                && !matches!(texture_cam.0.mode, Some(dcl_component::proto_components::sdk::components::pb_texture_camera::Mode::Orthographic(_)))
            {
                camera.insert(AtmosphereCamera {
                        render_layers: Some(render_layers.clone()),
                    }
                );
            }

            if maybe_layer.is_some_and(|l| {
                l.ambient_brightness_override.is_some() || l.ambient_color_override.is_some()
            }) {
                camera.insert(AmbientLight {
                    color: maybe_layer
                        .and_then(|l| l.ambient_color_override)
                        .map(Color3DclToBevy::convert_srgb)
                        .unwrap_or(global_light.ambient_color),
                    brightness: maybe_layer
                        .and_then(|l| l.ambient_brightness_override)
                        .unwrap_or(global_light.ambient_brightness)
                        * config.graphics.ambient_brightness as f32
                        * 20.0,
                    affects_lightmapped_meshes: false,
                });
            }

            // set audio receiver
            if texture_cam.0.volume() > 0.0 {
                camera.insert(AudioReceiver {
                    layers: render_layers,
                });
            }

            let camera_id = camera.id();

            commands
                .entity(ent)
                .try_push_children(&[camera_id])
                .insert((TextureCamEntity(camera_id), VideoTextureOutput(image)));

            new_cam_events.write(NewCameraEvent(camera_id));
        } else {
            // set active for current scenes only
            // TODO: limit / cycle
            let Some(existing) = maybe_existing_camera else {
                warn!("missing TextureCameraEntity");
                continue;
            };

            let Ok(camera) = cameras.get_mut(existing.0) else {
                warn!("missing camera entity for TextureCamera");
                continue;
            };

            let is_active = active_scenes.contains(&container.root);
            // debug!("[{}] active: {}", container.container_id, is_active);
            camera
                .map_unchanged(|c| &mut c.is_active)
                .set_if_neq(is_active);
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
        if let Ok(mut commands) = commands.get_entity(entity) {
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
