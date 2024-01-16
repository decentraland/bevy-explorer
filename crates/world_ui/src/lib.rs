use bevy::{
    pbr::{ExtendedMaterial, MaterialExtension, NotShadowCaster},
    prelude::*,
    render::{render_resource::{
        AsBindGroup, Extent3d, ShaderRef, ShaderType, TextureDimension, TextureFormat,
        TextureUsages,
    }, camera::RenderTarget},
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
    quad: Entity,
    image: Handle<Image>,
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

#[derive(Component)]
struct ProcessedWorldUi;

#[derive(Component)]
struct WorldUiCamera;

#[allow(clippy::too_many_arguments)]
fn update_world_ui(
    mut commands: Commands,
    q: Query<(Entity, &WorldUi), Or<(Changed<WorldUi>, Without<ProcessedWorldUi>)>>,
    mut wui: ResMut<WorldUiEntities>,
    mut removed: RemovedComponents<WorldUi>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TextShapeMaterial>>,
    mut camera_query: Query<(Entity, &mut Camera, &mut ResizeTarget), With<WorldUiCamera>>,
    quad_query: Query<&Handle<TextShapeMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut uis: Query<&mut Visibility>,
    mut current_rendered_ui: Local<Option<Entity>>,
) {
    // remove old quads
    for e in removed.read() {
        if let Some(entities) = wui.lookup.remove(&e) {
            if let Some(commands) = commands.get_entity(entities.quad) {
                commands.despawn_recursive();
            }
            if let Some(commands) = entities.ui.and_then(|ui| commands.get_entity(ui)) {
                commands.despawn_recursive();
            }
        }
    }

    // create cam if required
    let Ok((cam_ent, mut cam, mut target)) = camera_query.get_single_mut() else {
        commands.spawn((
            Camera2dBundle {
                camera: Camera {
                    order: -1,
                    is_active: false,
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
                width: None,
                height: None,
                info: ResizeInfo {
                    min_width: None,
                    max_width: None,
                    min_height: None,
                    max_height: None,
                    viewport_reference_size: UVec2::new(1024, 1024),
                },
            },
            WorldUiCamera,
        ));

        return;
    };

    cam.is_active = false;

    let mut new_uis = q.iter();

    // run one thing per frame as we reuse the camera
    if let Some((ent, ui)) = new_uis.next() {
        if let Some(ent) = current_rendered_ui.take() {
            if let Ok(mut vis) = uis.get_mut(ent) {
                *vis = Visibility::Hidden;
            }
        }
    
        if let Ok(mut vis) = uis.get_mut(ui.ui_root) {
            *vis = Visibility::Visible;
        }

        let image_size = Extent3d {
            width: if ui.resize_width.is_some() { 16 } else { ui.width.max(16) },
            height: if ui.resize_height.is_some() { 16 } else { ui.height.max(16) },
            depth_or_array_layers: 1,
        };

        let material_data = TextQuadData {
            valign: ui.valign,
            pix_per_m: ui.pix_per_m,
            add_y_pix: ui.add_y_pix,
        };

        // update camera
        cam.is_active = true;
        target.width = ui.resize_width;
        target.height = ui.resize_height;
        target.info.max_width = Some(ui.width);
        target.info.max_height = Some(ui.height);

        // create or update camera and quad
        let (quad, image) = if let Some(prev_items) = wui.lookup.get(&ent) {
            if let Ok(quad) = quad_query.get(prev_items.quad) {
                if let Some(mat) = materials.get_mut(quad) {
                    // update valign
                    mat.extension.data = material_data;

                    // update image
                    if let Some(image) =
                        images.get_mut(mat.base.base.base_color_texture.clone().unwrap())
                    {
                        let current_size = image.size();

                        let width_ok = ui.resize_width.is_some() || image_size.width == current_size.x;
                        let height_ok = ui.resize_height.is_some() || image_size.height == current_size.y;
                        if !width_ok || !height_ok {
                            image.resize(image_size);
                        }
                    }
                }
            }

            // dispose of previous ui if required
            if let Some(commands) = prev_items.ui.and_then(|e| commands.get_entity(e)) {
                commands.despawn_recursive();
            }

            (prev_items.quad, prev_items.image.clone())
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

            let quad = commands
                .spawn((
                    MaterialMeshBundle {
                        mesh: meshes.add(shape::Quad::default().into()),
                        material: materials.add(TextShapeMaterial {
                            base: SceneMaterial {
                                base: StandardMaterial {
                                    base_color_texture: Some(image.clone()),
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

            (quad, image)
        };

        if let Some(mut commands) = commands.get_entity(ui.ui_root) {
            cam.target = RenderTarget::Image(image.clone());
            cam.is_active = true;    
            commands.insert(TargetCamera(cam_ent));
            *current_rendered_ui = Some(ui.ui_root);
        }

        wui.lookup.insert(
            ent,
            WorldUiEntitySet {
                quad,
                image,
                ui: ui.dispose_ui.then_some(ui.ui_root),
            },
        );

        commands.entity(ent).try_insert(ProcessedWorldUi);
    }

    for (ent, wui) in new_uis {
        commands.entity(ent).remove::<ProcessedWorldUi>();
        if let Ok(mut vis) = uis.get_mut(wui.ui_root) {
            *vis = Visibility::Hidden;
        }
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
