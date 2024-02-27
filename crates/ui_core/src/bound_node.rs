use bevy::{
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderRef, ShaderType},
    ui::FocusPolicy,
    utils::HashMap,
    window::{PrimaryWindow, WindowResized},
};
use bevy_dui::{DuiRegistry, DuiTemplate};

use crate::combo_box::PropsExt;

#[derive(Component, Default)]
pub struct NodeBounds {
    pub corner_size: Val,
    pub corner_blend_size: Val,
    pub border_size: Val,
    pub border_color: Color,
}

#[derive(Component, Default, Clone, Debug)]
pub struct BoundedNode {
    pub image: Option<Handle<Image>>,
    pub color: Option<Color>,
}

#[derive(Bundle, Clone, Debug, Default)]
pub struct BoundedNodeBundle {
    pub bounded: BoundedNode,
    /// Describes the logical size of the node
    pub node: Node,
    /// Styles which control the layout (size and position) of the node and it's children
    /// In some cases these styles also affect how the node drawn/painted.
    pub style: Style,
    /// Whether this node should block interaction with lower nodes
    pub focus_policy: FocusPolicy,
    /// The transform of the node
    ///
    /// This component is automatically managed by the UI layout system.
    /// To alter the position of the `NodeBundle`, use the properties of the [`Style`] component.
    pub transform: Transform,
    /// The global transform of the node
    ///
    /// This component is automatically updated by the [`TransformPropagate`](`bevy_transform::TransformSystem::TransformPropagate`) systems.
    /// To alter the position of the `NodeBundle`, use the properties of the [`Style`] component.
    pub global_transform: GlobalTransform,
    /// Describes the visibility properties of the node
    pub visibility: Visibility,
    /// Inherited visibility of an entity.
    pub inherited_visibility: InheritedVisibility,
    /// Algorithmically-computed indication of whether an entity is visible and should be extracted for rendering
    pub view_visibility: ViewVisibility,
    /// Indicates the depth at which the node should appear in the UI
    pub z_index: ZIndex,
}

pub struct BoundedNodePlugin;

impl Plugin for BoundedNodePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiMaterialPlugin::<BoundedImageMaterial>::default())
            .add_systems(Startup, setup_templates)
            .add_systems(Update, update_bounded_nodes);
    }
}

#[derive(ShaderType, Debug, Clone)]
struct GpuBoundsData {
    bounds: Vec4,
    border_color: Vec4,
    corner_size: f32,
    corner_blend_size: f32,
    border_size: f32,
}

impl Default for GpuBoundsData {
    fn default() -> Self {
        Self {
            bounds: Vec4::new(f32::MIN, f32::MIN, f32::MAX, f32::MAX),
            border_color: Default::default(),
            corner_size: Default::default(),
            corner_blend_size: Default::default(),
            border_size: Default::default(),
        }
    }
}

#[derive(AsBindGroup, Asset, TypePath, Debug, Clone)]
struct BoundedImageMaterial {
    #[texture(0)]
    #[sampler(1)]
    image: Option<Handle<Image>>,
    #[uniform(2)]
    bounds: GpuBoundsData,
    #[uniform(3)]
    color: Vec4,
}

impl UiMaterial for BoundedImageMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/bound_node.wgsl".into()
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn update_bounded_nodes(
    mut commands: Commands,
    new_children: Query<
        (Entity, &BoundedNode),
        Or<(Without<Handle<BoundedImageMaterial>>, Changed<BoundedNode>)>,
    >,
    mut existing: Local<HashMap<Entity, Vec<(AssetId<BoundedImageMaterial>, bool)>>>,
    mut removed_nodes: RemovedComponents<Node>,
    mut mats: ResMut<Assets<BoundedImageMaterial>>,
    updated_nodes: Query<
        (Entity, &Node, &GlobalTransform, &NodeBounds),
        Or<(Changed<Node>, Changed<GlobalTransform>, Changed<NodeBounds>)>,
    >,
    all_nodes: Query<(Entity, &Node, &GlobalTransform, &NodeBounds)>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut resized: EventReader<WindowResized>,
    bound_parents: Query<(Option<&Parent>, Option<&NodeBounds>)>,
) {
    let Ok(window) = window.get_single() else {
        return;
    };
    let window = Vec2::new(window.width(), window.height());

    fn update_mat(
        mat: &mut BoundedImageMaterial,
        node: &Node,
        gt: &GlobalTransform,
        bounds: &NodeBounds,
        window: Vec2,
        add_border: bool,
    ) {
        let center = gt.translation().xy();
        let size = node.unrounded_size();
        mat.bounds.bounds = Vec4::new(
            center.x - size.x * 0.5,
            center.y - size.y * 0.5,
            center.x + size.x * 0.5,
            center.y + size.y * 0.5,
        );
        mat.bounds.corner_size = bounds
            .corner_size
            .resolve(node.unrounded_size().min_element(), window)
            .unwrap_or(0.0);
        mat.bounds.corner_blend_size = bounds
            .corner_blend_size
            .resolve(node.unrounded_size().min_element(), window)
            .unwrap_or(0.0);
        mat.bounds.border_size = bounds
            .border_size
            .resolve(node.unrounded_size().min_element(), window)
            .unwrap_or(0.0);
        if add_border {
            mat.bounds.border_color = bounds.border_color.as_linear_rgba_f32().into();
        } else {
            mat.bounds.border_color = Vec4::ZERO;
        }
        debug!("updated bounds: {:?}", mat);
    }

    let resolve_parent = |mut e: Entity| -> Option<Entity> {
        loop {
            let (maybe_parent, maybe_bounds) = bound_parents.get(e).ok()?;
            if maybe_bounds.is_some() {
                return Some(e);
            }
            e = maybe_parent.map(|p| p.get())?;
        }
    };

    for (ent, bound_node) in new_children.iter() {
        let color = bound_node.color.unwrap_or(if bound_node.image.is_some() {
            Color::WHITE
        } else {
            Color::NONE
        });
        let mut mat = BoundedImageMaterial {
            image: bound_node.image.clone(),
            color: color.as_linear_rgba_f32().into(),
            bounds: GpuBoundsData::default(),
        };
        let bound_parent = resolve_parent(ent);
        if let Some(bound_parent) = bound_parent.as_ref() {
            if let Ok((_, node, gt, bounds)) = all_nodes.get(*bound_parent) {
                update_mat(&mut mat, node, gt, bounds, window, *bound_parent == ent);
            };
        }

        let mat = mats.add(mat);

        if let Some(bound_parent) = bound_parent {
            existing
                .entry(bound_parent)
                .or_default()
                .push((mat.id(), bound_parent == ent));
        }
        commands.entity(ent).try_insert(mat);
    }

    for removed in removed_nodes.read() {
        existing.remove(&removed);
    }

    fn process<'a>(
        existing: &mut HashMap<Entity, Vec<(AssetId<BoundedImageMaterial>, bool)>>,
        mats: &mut Assets<BoundedImageMaterial>,
        window: Vec2,
        iter: impl Iterator<Item = (Entity, &'a Node, &'a GlobalTransform, &'a NodeBounds)>,
    ) {
        for (node_ent, node, gt, bounds) in iter {
            if let Some(ids) = existing.get_mut(&node_ent) {
                ids.retain(|(id, is_parent)| {
                    if let Some(mat) = mats.get_mut(*id) {
                        update_mat(mat, node, gt, bounds, window, *is_parent);
                        true
                    } else {
                        false
                    }
                });
            }
        }
    }

    if resized.read().last().is_some() {
        process(&mut existing, &mut mats, window, all_nodes.iter())
    } else {
        process(&mut existing, &mut mats, window, updated_nodes.iter())
    }
}

pub struct DuiNodeBounds;
impl DuiTemplate for DuiNodeBounds {
    fn render(
        &self,
        commands: &mut bevy::ecs::system::EntityCommands,
        mut props: bevy_dui::DuiProps,
        ctx: &mut bevy_dui::DuiContext,
    ) -> Result<bevy_dui::NodeMap, anyhow::Error> {
        commands.insert(NodeBounds {
            corner_size: props
                .take_as::<Val>(ctx, "corner-size")?
                .unwrap_or_default(),
            corner_blend_size: props.take_as::<Val>(ctx, "blend-size")?.unwrap_or_default(),
            border_size: props
                .take_as::<Val>(ctx, "border-size")?
                .unwrap_or_default(),
            border_color: props
                .take_as::<Color>(ctx, "border-color")?
                .unwrap_or_default(),
        });
        DuiBoundNode.render(commands, props, ctx)
    }
}

pub struct DuiBoundNode;
impl DuiTemplate for DuiBoundNode {
    fn render(
        &self,
        commands: &mut bevy::ecs::system::EntityCommands,
        mut props: bevy_dui::DuiProps,
        ctx: &mut bevy_dui::DuiContext,
    ) -> Result<bevy_dui::NodeMap, anyhow::Error> {
        let image = props.take_as::<Handle<Image>>(ctx, "bound-image")?;
        let color = props.take_as::<Color>(ctx, "color")?;
        commands.insert(BoundedNode { image, color });
        commands.remove::<BackgroundColor>();
        Ok(Default::default())
    }
}

fn setup_templates(mut dui: ResMut<DuiRegistry>) {
    dui.register_template("bounds", DuiNodeBounds);
    dui.register_template("bounded", DuiBoundNode);
}
