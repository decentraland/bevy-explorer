use bevy::{
    core::FrameCount,
    pbr::{ExtendedMaterial, MaterialExtension, NotShadowCaster},
    prelude::*,
    render::{
        camera::RenderTarget,
        render_asset::RenderAssetUsages,
        render_resource::{
            AsBindGroup, Extent3d, ShaderRef, ShaderType, TextureDimension, TextureFormat,
            TextureUsages,
        },
    },
    transform::TransformSystem,
    ui::UiSystem,
    utils::HashMap,
};
use common::{sets::SceneSets, structs::AppConfig, util::TryPushChildrenEx};
use scene_material::{SceneBound, SceneMaterial};

pub struct WorldUiPlugin;

impl Plugin for WorldUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<TextShapeMaterial>::default());
        app.add_systems(Update, add_worldui_materials.in_set(SceneSets::PostLoop));
        app.add_systems(
            PostUpdate,
            update_worldui_materials
                .after(UiSystem::Layout)
                .after(TransformSystem::TransformPropagate),
        );
    }
}

#[derive(Component)]
pub struct WorldUi {
    pub dbg: String,
    pub pix_per_m: f32,
    pub valign: f32,
    pub halign: f32,
    pub add_y_pix: f32,
    pub bounds: Vec4,
    pub view: Entity,
    pub ui_node: Entity,
}

pub fn spawn_world_ui_view(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    existing_image: Option<&Handle<Image>>,
) -> (Entity, Handle<Image>) {
    let image = existing_image.cloned().unwrap_or_else(|| {
        let mut image = Image::new_fill(
            Extent3d {
                width: 16,
                height: 16,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[0, 0, 0, 0],
            TextureFormat::Bgra8UnormSrgb,
            RenderAssetUsages::all(),
        );
        image.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT;
        images.add(image)
    });

    let camera = commands
        .spawn((
            image.clone(),
            Camera2dBundle {
                camera: Camera {
                    target: RenderTarget::Image(image.clone()),
                    order: -1,
                    is_active: true,
                    clear_color: bevy::render::camera::ClearColorConfig::Custom(Color::NONE),
                    ..Default::default()
                },
                ..Default::default()
            },
        ))
        .id();
    debug!("spawn");

    (camera, image)
}

#[derive(Component)]
pub struct WorldUiMaterialRef(AssetId<TextShapeMaterial>, AssetId<Image>);

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn add_worldui_materials(
    mut commands: Commands,
    q: Query<(Entity, &WorldUi, Option<&Children>), Changed<WorldUi>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TextShapeMaterial>>,
    config: Res<AppConfig>,
    targets: Query<&Handle<Image>>,
    mats: Query<&Handle<TextShapeMaterial>>,
    frame: Res<FrameCount>,
) {
    for (ent, wui, maybe_children) in q.iter() {
        let Ok(target) = targets.get(wui.view) else {
            warn!("world ui view not found");
            continue;
        };

        let material_data = TextQuadData {
            uvs: Vec4::ZERO,
            valign: wui.valign,
            halign: wui.halign,
            pix_per_m: wui.pix_per_m,
            add_y_pix: wui.add_y_pix,
        };

        let material = materials.add(TextShapeMaterial {
            base: SceneMaterial {
                base: StandardMaterial {
                    base_color: Color::srgb(2.0, 2.0, 2.0),
                    base_color_texture: Some(target.clone()),
                    unlit: true,
                    double_sided: true,
                    cull_mode: None,
                    alpha_mode: AlphaMode::Blend,
                    ..Default::default()
                },
                extension: SceneBound::new(wui.bounds, config.graphics.oob),
            },
            extension: TextQuad {
                data: material_data,
            },
        });

        commands
            .entity(wui.ui_node)
            .try_insert(WorldUiMaterialRef(material.id(), target.id()));

        let quad = commands
            .spawn((
                MaterialMeshBundle {
                    mesh: meshes.add(bevy::math::primitives::Rectangle::default().mesh()),
                    material,
                    ..Default::default()
                },
                NotShadowCaster,
            ))
            .id();

        // remove previous quads
        if let Some(children) = maybe_children {
            for &child in children {
                if mats.get(child).is_ok() {
                    commands.entity(child).despawn_recursive();
                }
            }
        }

        debug!("[{}] wui {} -> {:?}", frame.0, wui.dbg, wui.ui_node);

        commands.entity(ent).try_push_children(&[quad]);
    }
}

#[allow(clippy::type_complexity)]
pub fn update_worldui_materials(
    q: Query<
        (Entity, &WorldUiMaterialRef, &Node, &GlobalTransform),
        Or<(
            Changed<Node>,
            Changed<GlobalTransform>,
            Added<WorldUiMaterialRef>,
        )>,
    >,
    mut mats: ResMut<Assets<TextShapeMaterial>>,
    mut images: ResMut<Assets<Image>>,
    frame: Res<FrameCount>,
) {
    let mut target_sizes: HashMap<AssetId<Image>, UVec2> = HashMap::default();

    for (ent, ref_mat, node, gt) in q.iter() {
        let Some(mat) = mats.get_mut(ref_mat.0) else {
            warn!("failed to update mat");
            continue;
        };

        let translation = gt.translation();

        let topleft = translation.xy() - node.size() / 2.0;
        let bottomright = translation.xy() + node.size() / 2.0;
        mat.extension.data.uvs = Vec4::new(topleft.x, topleft.y, bottomright.x, bottomright.y);
        debug!(
            "[{}] img {:?}, {ent:?} uvs set to {} (size: {}, translation: {})",
            frame.0,
            ref_mat.1,
            mat.extension.data.uvs,
            node.size(),
            translation.xy()
        );

        let max_extent = target_sizes.entry(ref_mat.1).or_default();
        *max_extent = max_extent.max(bottomright.ceil().as_uvec2());
    }

    for (id, req_size) in target_sizes.into_iter() {
        let Some(image) = images.get(id) else {
            warn!("no image");
            continue;
        };

        if image.size().cmplt(req_size).any() {
            debug!("resized to {}", req_size);
            images.get_mut(id).unwrap().resize(Extent3d {
                width: req_size.x,
                height: req_size.y,
                depth_or_array_layers: 1,
            });
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
    pub uvs: Vec4,
    pub valign: f32,
    pub halign: f32,
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
