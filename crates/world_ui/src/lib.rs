use bevy::{
    diagnostic::FrameCount,
    pbr::{ExtendedMaterial, MaterialExtension, NotShadowCaster},
    platform::collections::{HashMap, HashSet},
    prelude::*,
    render::{
        camera::RenderTarget,
        render_asset::RenderAssetUsages,
        render_resource::{
            AsBindGroup, Extent3d, ShaderRef, TextureDimension, TextureFormat, TextureUsages,
        },
        renderer::RenderDevice,
        view::NoFrustumCulling,
    },
    transform::TransformSystem,
    ui::UiSystem,
};
use boimp::bake::{
    ImposterBakeMaterialExtension, ImposterBakeMaterialPlugin, STANDARD_BAKE_HANDLE,
};
use common::{
    sets::SceneSets,
    structs::{AppConfig, PreviewMode},
    util::TryPushChildrenEx,
};
use scene_material::{BoundRegion, SceneBound, SceneMaterial};

pub struct WorldUiPlugin;

impl Plugin for WorldUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<TextShapeMaterial>::default());
        if !app.world().resource::<PreviewMode>().is_preview {
            app.add_plugins(ImposterBakeMaterialPlugin::<TextShapeMaterial>::default());
        }

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
pub struct WorldUiRenderTarget(Handle<Image>);

#[derive(Component)]
pub struct WorldUi {
    pub dbg: String,
    pub pix_per_m: f32,
    pub valign: f32,
    pub halign: f32,
    pub add_y_pix: f32,
    pub bounds: Vec<BoundRegion>,
    pub view: Entity,
    pub ui_node: Entity,
    pub vertex_billboard: bool,
    pub blend_mode: AlphaMode,
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
        image.data = None;
        image.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT;
        images.add(image)
    });

    let camera = commands
        .spawn((
            WorldUiRenderTarget(image.clone()),
            Camera2d,
            Camera {
                target: RenderTarget::Image(image.clone().into()),
                order: -1,
                is_active: true,
                clear_color: bevy::render::camera::ClearColorConfig::Custom(Color::NONE),
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
    targets: Query<&WorldUiRenderTarget>,
    mats: Query<&MeshMaterial3d<TextShapeMaterial>>,
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
            vertex_billboard: if wui.vertex_billboard { 1 } else { 0 },
            _pad0: 0,
            _pad1: 0,
            _pad2: 0,
        };

        let material = materials.add(TextShapeMaterial {
            base: SceneMaterial {
                base: StandardMaterial {
                    base_color: Color::srgb(2.0, 2.0, 2.0),
                    base_color_texture: Some(target.0.clone()),
                    unlit: true,
                    alpha_mode: wui.blend_mode,
                    ..Default::default()
                },
                extension: SceneBound::new(wui.bounds.clone(), config.graphics.oob),
            },
            extension: TextQuad {
                data: material_data,
            },
        });

        commands
            .entity(wui.ui_node)
            .try_insert(WorldUiMaterialRef(material.id(), target.0.id()));

        let quad = commands
            .spawn((
                Mesh3d(meshes.add(bevy::math::primitives::Rectangle::default().mesh())),
                MeshMaterial3d(material),
                NotShadowCaster,
                NoFrustumCulling, // TODO calculate aabb based on font size (and update when it changes)
            ))
            .id();

        // remove previous quads
        if let Some(children) = maybe_children {
            for &child in children {
                if mats.get(child).is_ok() {
                    commands.entity(child).despawn();
                }
            }
        }

        debug!("[{}] wui {} -> {:?}", frame.0, wui.dbg, wui.ui_node);

        commands.entity(ent).try_push_children(&[quad]);
    }
}

#[allow(clippy::type_complexity)]
pub fn update_worldui_materials(
    changed: Query<
        &WorldUiMaterialRef,
        Or<(
            Changed<WorldUiMaterialRef>,
            Changed<ComputedNode>,
            Changed<GlobalTransform>,
        )>,
    >,
    all: Query<(Entity, &WorldUiMaterialRef, &ComputedNode, &GlobalTransform)>,
    mut mats: ResMut<Assets<TextShapeMaterial>>,
    mut images: ResMut<Assets<Image>>,
    frame: Res<FrameCount>,
    render_device: Res<RenderDevice>,
    mut prev_changed_targets: Local<HashSet<AssetId<Image>>>,
) {
    let mut changed_targets = std::mem::take(&mut *prev_changed_targets);
    changed_targets.extend(changed.iter().map(|mat| mat.1));

    if changed_targets.is_empty() {
        return;
    }

    let mut target_sizes: HashMap<AssetId<Image>, UVec2> = HashMap::new();

    for (ent, ref_mat, node, gt) in all.iter() {
        if !changed_targets.contains(&ref_mat.1) {
            continue;
        }

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

    *prev_changed_targets = target_sizes
        .into_iter()
        .filter_map(|(id, req_size)| {
            let Some(image) = images.get(id) else {
                warn!("no image");
                return None;
            };

            if image.size().cmplt(req_size).any() {
                let max_size = UVec2::splat(render_device.limits().max_texture_dimension_2d);
                if req_size.cmpge(max_size).any() {
                    warn!("too many textshapes, truncating image");
                    // TODO: split out to separate textures
                }
                let req_size = req_size.min(max_size).max(image.size());
                debug!("resized to {}", req_size);
                images.get_mut(id).unwrap().texture_descriptor.size = Extent3d {
                    width: req_size.x,
                    height: req_size.y,
                    depth_or_array_layers: 1,
                };
            }

            Some(id)
        })
        .collect();
}

pub type TextShapeMaterial = ExtendedMaterial<SceneMaterial, TextQuad>;

#[derive(Asset, TypePath, Clone, AsBindGroup)]
pub struct TextQuad {
    #[uniform(200)]
    pub data: TextQuadData,
}

mod decl {
    #![allow(dead_code)]

    use bevy::{math::Vec4, render::render_resource::ShaderType};
    #[derive(Clone, ShaderType)]
    pub struct TextQuadData {
        pub uvs: Vec4,
        pub valign: f32,
        pub halign: f32,
        pub pix_per_m: f32,
        pub add_y_pix: f32,
        pub vertex_billboard: u32,
        pub(super) _pad0: u32,
        pub(super) _pad1: u32,
        pub(super) _pad2: u32,
    }
}
pub use decl::*;

impl MaterialExtension for TextQuad {
    fn vertex_shader() -> bevy::render::render_resource::ShaderRef {
        ShaderRef::Path("embedded://shaders/text_quad_vertex.wgsl".into())
    }

    fn prepass_vertex_shader() -> ShaderRef {
        ShaderRef::Path("embedded://shaders/text_quad_vertex.wgsl".into())
    }
}

impl ImposterBakeMaterialExtension for TextQuad {
    fn imposter_fragment_shader() -> bevy::render::render_resource::ShaderRef {
        STANDARD_BAKE_HANDLE.into()
    }
}
