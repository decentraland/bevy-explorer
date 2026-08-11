use anyhow::{anyhow, bail};
use bevy::{
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderRef},
    ui::UiSystem,
};
use bevy_dui::{DuiContext, DuiProps, DuiRegistry, DuiTemplate, NodeMap};
use bevy_ecss::StyleSheetAsset;
use common::asset_cache::{clean_asset_cache, AssetCache};

/// specify a background image using 9-slice scaling
/// https://en.wikipedia.org/wiki/9-slice_scaling
/// must be added to an entity with `NodeBundle` components
#[derive(Component, Default)]
pub struct Ui9Slice {
    /// the image to be sliced
    pub image: Handle<Image>,
    /// rect defining the edges of the center / stretched region
    /// Val::Px uses so many pixels
    /// Val::Percent uses a percent of the image size
    /// Val::Auto and Val::Undefined are treated as zero.
    pub center_region: UiRect,
    pub tint: Option<Color>,
}

impl Ui9Slice {
    pub fn new(image: Handle<Image>, center_region: UiRect, tint: Option<Color>) -> Self {
        Self {
            image,
            center_region,
            tint,
        }
    }
}

#[derive(SystemSet, Hash, PartialEq, Eq, PartialOrd, Ord, Debug, Clone)]
pub struct Ui9SliceSet;

pub struct Ui9SlicePlugin;

impl Plugin for Ui9SlicePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup);
        app.add_plugins(UiMaterialPlugin::<NineSliceMaterial>::default());
        app.init_resource::<AssetCache<NineSliceKey, NineSliceMaterial>>();
        app.add_systems(
            PostUpdate,
            (
                update_slices.after(UiSystem::Layout),
                clean_asset_cache::<NineSliceKey, NineSliceMaterial>.after(update_slices),
            ),
        );
    }
}

#[derive(Component)]
struct Retry9Slice;

#[derive(PartialEq, Eq, Hash, Clone)]
struct NineSliceKey {
    image: AssetId<Image>,
    bounds: [u32; 4],
    surface: [u32; 2],
    color: [u32; 4],
}

// quantize to whole pixels so equal-sized nodes share a material
fn whole_pixel_size(node: &ComputedNode) -> Vec2 {
    node.unrounded_size().round()
}

#[allow(clippy::type_complexity)]
fn update_slices(
    mut commands: Commands,
    images: Res<Assets<Image>>,
    new_slices: Query<
        (
            Entity,
            &ComputedNode,
            &Ui9Slice,
            Option<&MaterialNode<NineSliceMaterial>>,
        ),
        Or<(
            Changed<Ui9Slice>,
            Added<Ui9Slice>,
            Changed<ComputedNode>,
            With<Retry9Slice>,
        )>,
    >,
    mut removed: RemovedComponents<Ui9Slice>,
    mut mats: ResMut<Assets<NineSliceMaterial>>,
    mut cache: ResMut<AssetCache<NineSliceKey, NineSliceMaterial>>,
) {
    // clean up removed slices
    for ent in removed.read() {
        if let Ok(mut commands) = commands.get_entity(ent) {
            commands.remove::<MaterialNode<NineSliceMaterial>>();
        }
    }

    for (ent, node, slice, maybe_material) in new_slices.iter() {
        let Some(image_size) = images.get(&slice.image).map(Image::size_f32) else {
            commands.entity(ent).try_insert(Retry9Slice);
            continue;
        };
        commands.entity(ent).remove::<Retry9Slice>();

        let bounds = Vec4::new(
            slice
                .center_region
                .left
                .resolve(image_size.x, Vec2::ZERO)
                .unwrap_or(0.0),
            slice
                .center_region
                .left
                .resolve(image_size.x, Vec2::ZERO)
                .unwrap_or(0.0),
            slice
                .center_region
                .left
                .resolve(image_size.x, Vec2::ZERO)
                .unwrap_or(0.0),
            slice
                .center_region
                .left
                .resolve(image_size.x, Vec2::ZERO)
                .unwrap_or(0.0),
        );
        let surface = whole_pixel_size(node);
        let color = slice.tint.unwrap_or(Color::WHITE).to_linear().to_vec4();

        let key = NineSliceKey {
            image: slice.image.id(),
            bounds: bounds.to_array().map(f32::to_bits),
            surface: surface.to_array().map(f32::to_bits),
            color: color.to_array().map(f32::to_bits),
        };

        let material = cache.get_or_add(key, &mut mats, || NineSliceMaterial {
            image: slice.image.clone(),
            bounds: GpuSliceData {
                bounds,
                surface: surface.extend(0.0).extend(0.0),
            },
            color,
        });

        // materials are shared, so swap handles instead of mutating in place
        match maybe_material {
            Some(existing) if existing.0.id() == material.id() => (),
            Some(_) => {
                commands.entity(ent).try_insert(MaterialNode(material));
            }
            None => {
                commands
                    .entity(ent)
                    .try_insert(MaterialNode(material))
                    .remove::<BackgroundColor>();
            }
        }
    }
}

mod decl {
    #![allow(dead_code)]
    use bevy::{math::Vec4, render::render_resource::ShaderType};
    #[derive(ShaderType, Debug, Clone)]
    pub(super) struct GpuSliceData {
        pub(super) bounds: Vec4,
        pub(super) surface: Vec4,
    }
}
use decl::*;

#[derive(AsBindGroup, Asset, TypePath, Debug, Clone)]
struct NineSliceMaterial {
    #[texture(0)]
    #[sampler(1)]
    image: Handle<Image>,
    #[uniform(2)]
    bounds: GpuSliceData,
    #[uniform(3)]
    color: Vec4,
}

impl UiMaterial for NineSliceMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://shaders/nineslice_material.wgsl".into()
    }
}

pub fn setup(mut dui: ResMut<DuiRegistry>) {
    dui.register_template("nineslice", Ui9SliceTemplate);
}

pub struct Ui9SliceTemplate;
impl DuiTemplate for Ui9SliceTemplate {
    fn render(
        &self,
        commands: &mut bevy::ecs::system::EntityCommands,
        mut props: DuiProps,
        ctx: &mut DuiContext,
    ) -> Result<NodeMap, anyhow::Error> {
        let border = props
            .take::<String>("slice-border")?
            .ok_or(anyhow!("no slice-border specified"))?;

        let image = match (
            props.borrow::<String>("slice-image", ctx),
            props.borrow::<Handle<Image>>("slice-image", ctx),
        ) {
            (Ok(Some(img)), _) => ctx.asset_server().load(img),
            (_, Ok(Some(handle))) => handle.clone(),
            _ => bail!("no slice-image specified"),
        };

        let tint = props.take::<String>("slice-color")?;

        let border_sheet = if let Some(tint) = tint.as_ref() {
            format!("#whatever {{ border: {border}; color: {tint}; }}")
        } else {
            format!("#whatever {{ border: {border}; }}")
        };

        let sheet = StyleSheetAsset::parse("", &border_sheet);
        let properties = &sheet.iter().next().unwrap().properties;

        let center_region = properties
            .get("border")
            .unwrap()
            .rect()
            .ok_or(anyhow!("failed to parse slice-border value `{border}`"))?;
        let tint: Option<Color> = if let Some(color) = properties.get("color") {
            Some(color.color().ok_or(anyhow!(
                "failed to parse slice-color value `{}`",
                tint.unwrap()
            ))?)
        } else {
            None
        };

        debug!("border rect: {center_region:?}");

        commands.try_insert((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(0.0),
                right: Val::Px(0.0),
                left: Val::Px(0.0),
                bottom: Val::Px(0.0),
                ..Default::default()
            },
            Ui9Slice {
                image,
                center_region,
                tint,
            },
        ));

        Ok(NodeMap::default())
    }
}
