use bevy::{
    pbr::{ExtendedMaterial, MaterialExtension, NotShadowCaster},
    prelude::*,
    render::render_resource::{
        AsBindGroup, Extent3d, ShaderRef, ShaderType, TextureDimension, TextureFormat,
        TextureUsages,
    },
    utils::HashMap,
};
use common::{sets::SceneSets, util::TryPushChildrenEx};
use scene_material::{SceneBound, SceneMaterial};

#[derive(SystemSet, Hash, Eq, PartialEq, Clone, Copy, Debug)]
pub struct WorldUiSet;

pub struct WorldUiPlugin;

impl Plugin for WorldUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldUiEntities>();
        app.add_plugins(MaterialPlugin::<TextShapeMaterial>::default());
        app.configure_sets(Update, WorldUiSet.in_set(SceneSets::PostLoop));
        app.add_systems(Update, update_world_ui.in_set(WorldUiSet));
    }
}

struct WorldUiEntitySet {
    camera: Entity,
    quad: Entity,
    ui: Option<Entity>,
}

#[derive(Resource, Default)]
pub struct WorldUiEntities {
    lookup: HashMap<Entity, WorldUiEntitySet>,
}

#[derive(Component)]
pub struct WorldUi {
    pub width: u32,
    pub height: u32,
    pub resize_width: Option<ResizeAxis>,
    pub resize_height: Option<ResizeAxis>,
    pub pix_per_m: f32,
    pub valign: f32,
    pub add_y_pix: f32,
    pub bounds: Vec4,
    pub ui_root: Entity,
    pub dispose_ui: bool,
}

#[allow(clippy::too_many_arguments)]
fn update_world_ui(
    mut commands: Commands,
    q: Query<(Entity, &WorldUi), Changed<WorldUi>>,
    mut wui: ResMut<WorldUiEntities>,
    mut removed: RemovedComponents<WorldUi>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TextShapeMaterial>>,
    mut camera_query: Query<(&mut Camera, &mut ResizeTarget)>,
    quad_query: Query<&Handle<TextShapeMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    // remove old cams
    for e in removed.read() {
        if let Some(entities) = wui.lookup.remove(&e) {
            if let Some(commands) = commands.get_entity(entities.camera) {
                commands.despawn_recursive();
            }
            if let Some(commands) = commands.get_entity(entities.quad) {
                commands.despawn_recursive();
            }
            if let Some(commands) = entities.ui.and_then(|ui| commands.get_entity(ui)) {
                commands.despawn_recursive();
            }
        }
    }

    //deactivate all cams after 1 frame (we will reactivate if required)
    for (mut cam, _) in camera_query.iter_mut() {
        cam.is_active = false;
    }

    for (ent, ui) in q.iter() {
        let image_size = Extent3d {
            width: ui.width,
            height: ui.height,
            depth_or_array_layers: 1,
        };

        let material_data = TextQuadData {
            valign: ui.valign,
            pix_per_m: ui.pix_per_m,
            add_y_pix: ui.add_y_pix,
        };

        // create or update camera and quad
        let (camera, quad) = if let Some(prev_items) = wui.lookup.get(&ent) {
            // update camera
            if let Ok((mut cam, mut target)) = camera_query.get_mut(prev_items.camera) {
                cam.is_active = true;
                target.width = ui.resize_width;
                target.height = ui.resize_height;
            }

            if let Ok(quad) = quad_query.get(prev_items.quad) {
                if let Some(mat) = materials.get_mut(quad) {
                    // update valign
                    mat.extension.data = material_data;

                    // update image
                    if let Some(image) =
                        images.get_mut(mat.base.base.base_color_texture.clone().unwrap())
                    {
                        image.resize(image_size)
                    }
                }
            }

            // dispose of previous ui if required
            if let Some(commands) = prev_items.ui.and_then(|e| commands.get_entity(e)) {
                commands.despawn_recursive();
            }

            (prev_items.camera, prev_items.quad)
        } else {
            // create render target image (it'll be resized)
            let mut image = Image::new_fill(
                image_size,
                TextureDimension::D2,
                &[0, 0, 0, 0],
                TextureFormat::Bgra8UnormSrgb,
            );
            image.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT;
            let image = images.add(image);

            let camera = commands
                .spawn((
                    Camera2dBundle {
                        camera: Camera {
                            order: -1,
                            target: bevy::render::camera::RenderTarget::Image(image.clone()),
                            ..Default::default()
                        },
                        camera_2d: Camera2d {
                            clear_color: bevy::core_pipeline::clear_color::ClearColorConfig::Custom(
                                Color::NONE,
                            ),
                        },
                        ..Default::default()
                    },
                    ResizeTarget {
                        width: ui.resize_width,
                        height: ui.resize_height,
                        info: ResizeInfo {
                            min_width: None,
                            max_width: Some(ui.width),
                            min_height: None,
                            max_height: Some(ui.height),
                            viewport_reference_size: UVec2::new(1024, 1024),
                        },
                    },
                ))
                .id();

            let quad = commands
                .spawn((
                    MaterialMeshBundle {
                        mesh: meshes.add(shape::Quad::default().into()),
                        material: materials.add(TextShapeMaterial {
                            base: SceneMaterial {
                                base: StandardMaterial {
                                    base_color_texture: Some(image),
                                    unlit: true,
                                    double_sided: true,
                                    cull_mode: None,
                                    alpha_mode: AlphaMode::Blend,
                                    ..Default::default()
                                },
                                extension: SceneBound { bounds: ui.bounds },
                            },
                            extension: TextQuad {
                                data: material_data,
                            },
                        }),
                        ..Default::default()
                    },
                    NotShadowCaster,
                ))
                .id();

            commands.entity(ent).try_push_children(&[quad]);

            (camera, quad)
        };

        if let Some(mut commands) = commands.get_entity(ui.ui_root) {
            commands.insert(TargetCamera(camera));
        }

        wui.lookup.insert(ent, WorldUiEntitySet { camera, quad, ui: ui.dispose_ui.then_some(ui.ui_root) });
    }
}

pub type TextShapeMaterial = ExtendedMaterial<SceneMaterial, TextQuad>;

#[derive(Asset, TypePath, Clone, AsBindGroup)]
pub struct TextQuad {
    #[uniform(200)]
    pub data: TextQuadData,
}

#[derive(Clone, ShaderType)]
pub struct TextQuadData {
    pub valign: f32,
    pub pix_per_m: f32,
    pub add_y_pix: f32,
}

impl MaterialExtension for TextQuad {
    fn vertex_shader() -> bevy::render::render_resource::ShaderRef {
        ShaderRef::Path("shaders/text_quad_vertex.wgsl".into())
    }

    fn prepass_vertex_shader() -> ShaderRef {
        ShaderRef::Path("shaders/text_quad_vertex.wgsl".into())
    }
}
