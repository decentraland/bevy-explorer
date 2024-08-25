use std::collections::{BTreeMap, BTreeSet};

use bevy::{
    math::FloatOrd,
    prelude::*,
    render::render_resource::Extent3d,
    ui::{FocusPolicy, ManualCursorPosition},
    utils::{HashMap, HashSet},
};
use bevy_dui::{DuiCommandsExt, DuiProps, DuiRegistry};

use crate::{
    renderer_context::RendererSceneContext, update_scene::pointer_results::UiPointerTarget,
    update_world::text_shape::make_text_section, ContainerEntity, ContainingScene, SceneEntity,
    SceneSets,
};
use common::{
    structs::{AppConfig, PrimaryUser},
    util::{DespawnWith, FireEventEx, ModifyComponentExt, TryPushChildrenEx},
};
use dcl::interface::{ComponentPosition, CrdtType};
use dcl_component::{
    proto_components::{
        self,
        common::{texture_union, BorderRect, TextureUnion},
        sdk::components::{
            self, scroll_position_value, PbUiBackground, PbUiCanvas, PbUiDropdown,
            PbUiDropdownResult, PbUiInput, PbUiInputResult, PbUiScrollResult, PbUiText,
            PbUiTransform, ScrollPositionValue, TextWrap, YgAlign, YgDisplay, YgFlexDirection,
            YgJustify, YgOverflow, YgPositionType, YgUnit, YgWrap,
        },
    },
    SceneComponentId, SceneEntityId,
};
use ui_core::{
    combo_box::ComboBox,
    nine_slice::Ui9Slice,
    scrollable::{
        ScrollDirection, ScrollPosition, ScrollTarget, ScrollTargetEvent, Scrollable, StartPosition,
    },
    stretch_uvs_image::StretchUvMaterial,
    textentry::TextEntry,
    ui_actions::{DataChanged, HoverEnter, HoverExit, On, Submit, UiCaller},
    ui_builder::SpawnSpacer,
};

use super::{material::TextureResolver, pointer_events::PointerEvents, AddCrdtInterfaceExt};

pub struct SceneUiPlugin;

#[derive(Debug, Copy, Clone)]
struct Size {
    width: Val,
    height: Val,
}

#[derive(Debug, Copy, Clone)]
struct MaybeSize {
    width: Option<Val>,
    height: Option<Val>,
}

// macro helpers to convert proto format to bevy format for val, size, rect
macro_rules! val {
    ($pb:ident, $u:ident, $v:ident, $d:expr) => {
        match $pb.$u() {
            YgUnit::YguUndefined => $d,
            YgUnit::YguAuto => Val::Auto,
            YgUnit::YguPoint => Val::Px($pb.$v),
            YgUnit::YguPercent => Val::Percent($pb.$v),
        }
    };
}

macro_rules! size {
    ($pb:ident, $wu:ident, $w:ident, $hu:ident, $h:ident, $d:expr) => {{
        Size {
            width: val!($pb, $wu, $w, $d),
            height: val!($pb, $hu, $h, $d),
        }
    }};
}

// macro helpers to convert proto format to bevy format for val, size, rect
macro_rules! maybe_val {
    ($pb:ident, $u:ident, $v:ident, $d:expr) => {
        match $pb.$u() {
            YgUnit::YguUndefined => None,
            YgUnit::YguAuto => Some(Val::Auto),
            YgUnit::YguPoint => Some(Val::Px($pb.$v)),
            YgUnit::YguPercent => Some(Val::Percent($pb.$v)),
        }
    };
}

macro_rules! maybe_size {
    ($pb:ident, $wu:ident, $w:ident, $hu:ident, $h:ident, $d:expr) => {{
        MaybeSize {
            width: maybe_val!($pb, $wu, $w, $d),
            height: maybe_val!($pb, $hu, $h, $d),
        }
    }};
}

macro_rules! rect {
    ($pb:ident, $lu:ident, $l:ident, $ru:ident, $r:ident, $tu:ident, $t:ident, $bu:ident, $b:ident, $d:expr) => {
        UiRect {
            left: val!($pb, $lu, $l, $d),
            right: val!($pb, $ru, $r, $d),
            top: val!($pb, $tu, $t, $d),
            bottom: val!($pb, $bu, $b, $d),
        }
    };
}

#[derive(Component, Debug, Clone)]
pub struct UiTransform {
    element_id: Option<String>,
    parent: SceneEntityId,
    right_of: SceneEntityId,
    align_content: AlignContent,
    align_items: AlignItems,
    wrap: FlexWrap,
    shrink: f32,
    position_type: PositionType,
    align_self: AlignSelf,
    flex_direction: FlexDirection,
    justify_content: JustifyContent,
    overflow: Overflow,
    scroll: bool,
    scroll_h_visible: bool,
    scroll_v_visible: bool,
    scroll_position: Option<ScrollPositionValue>,
    display: Display,
    basis: Val,
    grow: f32,
    size: MaybeSize,
    min_size: Size,
    max_size: Size,
    position: UiRect,
    margin: UiRect,
    padding: UiRect,
    opacity: f32,
}

impl From<PbUiTransform> for UiTransform {
    fn from(value: PbUiTransform) -> Self {
        Self {
            // debug: value.clone(),
            element_id: value.element_id.clone(),
            parent: SceneEntityId::from_proto_u32(value.parent as u32),
            right_of: SceneEntityId::from_proto_u32(value.right_of as u32),
            align_content: match value.align_content() {
                YgAlign::YgaAuto |
                YgAlign::YgaBaseline | // baseline is invalid for align content
                YgAlign::YgaStretch => AlignContent::Stretch,
                YgAlign::YgaFlexStart => AlignContent::FlexStart,
                YgAlign::YgaCenter => AlignContent::Center,
                YgAlign::YgaFlexEnd => AlignContent::FlexEnd,
                YgAlign::YgaSpaceBetween => AlignContent::SpaceBetween,
                YgAlign::YgaSpaceAround => AlignContent::SpaceAround,
            },
            align_items: match value.align_items() {
                YgAlign::YgaAuto |
                YgAlign::YgaSpaceBetween | // invalid
                YgAlign::YgaSpaceAround | // invalid
                YgAlign::YgaStretch => AlignItems::Stretch,
                YgAlign::YgaFlexStart => AlignItems::FlexStart,
                YgAlign::YgaCenter => AlignItems::Center,
                YgAlign::YgaFlexEnd => AlignItems::FlexEnd,
                YgAlign::YgaBaseline => AlignItems::Baseline,
            },
            wrap: match value.flex_wrap() {
                YgWrap::YgwNoWrap => FlexWrap::NoWrap,
                YgWrap::YgwWrap => FlexWrap::Wrap,
                YgWrap::YgwWrapReverse => FlexWrap::WrapReverse,
            },
            shrink: value.flex_shrink.unwrap_or(1.0),
            position_type: match value.position_type() {
                YgPositionType::YgptRelative => PositionType::Relative,
                YgPositionType::YgptAbsolute => PositionType::Absolute,
            },
            align_self: match value.align_self() {
                YgAlign::YgaSpaceBetween | // invalid
                YgAlign::YgaSpaceAround | // invalid
                YgAlign::YgaAuto => AlignSelf::Auto,
                YgAlign::YgaFlexStart => AlignSelf::FlexStart,
                YgAlign::YgaCenter => AlignSelf::Center,
                YgAlign::YgaFlexEnd => AlignSelf::FlexEnd,
                YgAlign::YgaStretch => AlignSelf::Stretch,
                YgAlign::YgaBaseline => AlignSelf::Baseline,
            },
            flex_direction: match value.flex_direction() {
                YgFlexDirection::YgfdRow => FlexDirection::Row,
                YgFlexDirection::YgfdColumn => FlexDirection::Column,
                YgFlexDirection::YgfdColumnReverse => FlexDirection::ColumnReverse,
                YgFlexDirection::YgfdRowReverse => FlexDirection::RowReverse,
            },
            justify_content: match value.justify_content() {
                YgJustify::YgjFlexStart => JustifyContent::FlexStart,
                YgJustify::YgjCenter => JustifyContent::Center,
                YgJustify::YgjFlexEnd => JustifyContent::FlexEnd,
                YgJustify::YgjSpaceBetween => JustifyContent::SpaceBetween,
                YgJustify::YgjSpaceAround => JustifyContent::SpaceAround,
                YgJustify::YgjSpaceEvenly => JustifyContent::SpaceEvenly,
            },
            overflow: match value.overflow() {
                YgOverflow::YgoVisible => Overflow::DEFAULT,
                YgOverflow::YgoHidden => Overflow::clip(),
                YgOverflow::YgoScroll => Overflow::clip(),
            },
            scroll: value.overflow() == YgOverflow::YgoScroll,
            scroll_position: value.scroll_position.clone(),
            scroll_h_visible: [
                components::ShowScrollBar::SsbBoth,
                components::ShowScrollBar::SsbOnlyHorizontal,
            ]
            .contains(&value.scroll_visible()),
            scroll_v_visible: [
                components::ShowScrollBar::SsbBoth,
                components::ShowScrollBar::SsbOnlyVertical,
            ]
            .contains(&value.scroll_visible()),
            display: match value.display() {
                YgDisplay::YgdFlex => Display::Flex,
                YgDisplay::YgdNone => Display::None,
            },
            basis: val!(value, flex_basis_unit, flex_basis, Val::Auto),
            grow: value.flex_grow,
            size: maybe_size!(value, width_unit, width, height_unit, height, Val::Auto),
            min_size: size!(
                value,
                min_width_unit,
                min_width,
                min_height_unit,
                min_height,
                Val::Auto
            ),
            max_size: size!(
                value,
                max_width_unit,
                max_width,
                max_height_unit,
                max_height,
                Val::Auto
            ),
            position: rect!(
                value,
                position_left_unit,
                position_left,
                position_right_unit,
                position_right,
                position_top_unit,
                position_top,
                position_bottom_unit,
                position_bottom,
                Val::Auto
            ),
            margin: rect!(
                value,
                margin_left_unit,
                margin_left,
                margin_right_unit,
                margin_right,
                margin_top_unit,
                margin_top,
                margin_bottom_unit,
                margin_bottom,
                Val::Px(0.0)
            ),
            padding: rect!(
                value,
                padding_left_unit,
                padding_left,
                padding_right_unit,
                padding_right,
                padding_top_unit,
                padding_top,
                padding_bottom_unit,
                padding_bottom,
                Val::Px(0.0)
            ),
            opacity: value.opacity.unwrap_or(1.0),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum BackgroundTextureMode {
    NineSlices(BorderRect),
    Stretch([Vec4; 2]),
    Center,
}

impl BackgroundTextureMode {
    pub fn stretch_default() -> Self {
        Self::Stretch([Vec4::W, Vec4::ONE - Vec4::W])
    }
}

#[derive(Component, Clone, Debug)]
pub struct BackgroundTexture {
    tex: TextureUnion,
    mode: BackgroundTextureMode,
}

#[derive(Component, Clone, Debug)]
pub struct UiBackground {
    color: Option<Color>,
    texture: Option<BackgroundTexture>,
}

impl From<PbUiBackground> for UiBackground {
    fn from(value: PbUiBackground) -> Self {
        let texture_mode = value.texture_mode();
        Self {
            color: value.color.map(Into::into),
            texture: value.texture.map(|tex| {
                let mode = match texture_mode {
                    components::BackgroundTextureMode::NineSlices => {
                        BackgroundTextureMode::NineSlices(value.texture_slices.unwrap_or(
                            BorderRect {
                                top: 1.0 / 3.0,
                                bottom: 1.0 / 3.0,
                                left: 1.0 / 3.0,
                                right: 1.0 / 3.0,
                            },
                        ))
                    }
                    components::BackgroundTextureMode::Center => BackgroundTextureMode::Center,
                    components::BackgroundTextureMode::Stretch => {
                        // the uvs array contain [tl.x, tl.y, bl.x, bl.y, br.x, br.y, tr.x, tr.y]
                        let mut iter = value.uvs.iter().copied();
                        let uvs = [
                            Vec4::new(
                                iter.next().unwrap_or(0.0),
                                iter.next().unwrap_or(0.0),
                                iter.next().unwrap_or(0.0),
                                iter.next().unwrap_or(1.0),
                            ),
                            Vec4::new(
                                iter.next().unwrap_or(1.0),
                                iter.next().unwrap_or(1.0),
                                iter.next().unwrap_or(1.0),
                                iter.next().unwrap_or(0.0),
                            ),
                        ];
                        BackgroundTextureMode::Stretch(uvs)
                    }
                };

                BackgroundTexture { tex, mode }
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum VAlign {
    Top,
    Middle,
    Bottom,
}

#[derive(Component, Clone, Debug)]
pub struct UiText {
    pub text: String,
    pub color: Color,
    pub h_align: JustifyText,
    pub v_align: VAlign,
    pub font: proto_components::sdk::components::common::Font,
    pub font_size: f32,
    pub wrapping: bool,
}

impl From<PbUiText> for UiText {
    fn from(value: PbUiText) -> Self {
        let text_align = value
            .text_align
            .map(|_| value.text_align())
            .unwrap_or(components::common::TextAlignMode::TamMiddleCenter);

        Self {
            text: value.value.clone(),
            color: value.color.map(Into::into).unwrap_or(Color::WHITE),
            h_align: match text_align {
                components::common::TextAlignMode::TamTopLeft
                | components::common::TextAlignMode::TamMiddleLeft
                | components::common::TextAlignMode::TamBottomLeft => JustifyText::Left,
                components::common::TextAlignMode::TamTopCenter
                | components::common::TextAlignMode::TamMiddleCenter
                | components::common::TextAlignMode::TamBottomCenter => JustifyText::Center,
                components::common::TextAlignMode::TamTopRight
                | components::common::TextAlignMode::TamMiddleRight
                | components::common::TextAlignMode::TamBottomRight => JustifyText::Right,
            },
            v_align: match text_align {
                components::common::TextAlignMode::TamTopLeft
                | components::common::TextAlignMode::TamTopCenter
                | components::common::TextAlignMode::TamTopRight => VAlign::Top,
                components::common::TextAlignMode::TamMiddleLeft
                | components::common::TextAlignMode::TamMiddleCenter
                | components::common::TextAlignMode::TamMiddleRight => VAlign::Middle,
                components::common::TextAlignMode::TamBottomLeft
                | components::common::TextAlignMode::TamBottomCenter
                | components::common::TextAlignMode::TamBottomRight => VAlign::Bottom,
            },
            font: value.font(),
            font_size: value.font_size.unwrap_or(10) as f32,
            wrapping: value.text_wrap() == TextWrap::TwWrap,
        }
    }
}

#[derive(Component, Clone)]
pub struct UiCanvas(pub PbUiCanvas);

impl From<PbUiCanvas> for UiCanvas {
    fn from(value: PbUiCanvas) -> Self {
        Self(value)
    }
}

#[derive(Component, Debug)]
pub struct UiInput(PbUiInput);

impl From<PbUiInput> for UiInput {
    fn from(value: PbUiInput) -> Self {
        Self(value)
    }
}

#[derive(Component, Debug)]
pub struct UiDropdown(PbUiDropdown);

impl From<PbUiDropdown> for UiDropdown {
    fn from(value: PbUiDropdown) -> Self {
        Self(value)
    }
}

impl Plugin for SceneUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbUiTransform, UiTransform>(
            SceneComponentId::UI_TRANSFORM,
            ComponentPosition::EntityOnly,
        );
        app.add_crdt_lww_component::<PbUiBackground, UiBackground>(
            SceneComponentId::UI_BACKGROUND,
            ComponentPosition::EntityOnly,
        );
        app.add_crdt_lww_component::<PbUiText, UiText>(
            SceneComponentId::UI_TEXT,
            ComponentPosition::EntityOnly,
        );
        app.add_crdt_lww_component::<PbUiInput, UiInput>(
            SceneComponentId::UI_INPUT,
            ComponentPosition::EntityOnly,
        );
        app.add_crdt_lww_component::<PbUiDropdown, UiDropdown>(
            SceneComponentId::UI_DROPDOWN,
            ComponentPosition::EntityOnly,
        );
        app.add_crdt_lww_component::<PbUiCanvas, UiCanvas>(
            SceneComponentId::UI_CANVAS,
            ComponentPosition::EntityOnly,
        );

        app.add_systems(Update, init_scene_ui_root.in_set(SceneSets::PostInit));
        app.add_systems(
            Update,
            (
                update_scene_ui_components,
                create_ui_roots,
                layout_scene_ui,
                (
                    set_ui_background,
                    set_ui_input,
                    set_ui_dropdown,
                    set_ui_pointer_events,
                    set_ui_text,
                ),
                fully_update_target_camera_system,
            )
                .chain()
                .in_set(SceneSets::PostLoop),
        );
    }
}

#[derive(Component, Default)]
pub struct SceneUiData {
    nodes: BTreeSet<Entity>,
    relayout: bool,
}

#[derive(Component)]
pub struct UiTextureOutput {
    pub camera: Entity,
    pub image: Handle<Image>,
    pub texture_size: UVec2,
}

fn init_scene_ui_root(
    mut commands: Commands,
    scenes: Query<Entity, (With<RendererSceneContext>, Without<SceneUiData>)>,
) {
    for scene_ent in scenes.iter() {
        commands
            .entity(scene_ent)
            .try_insert(SceneUiData::default());
    }
}

#[allow(clippy::type_complexity)]
fn update_scene_ui_components(
    changed_entities: Query<(Entity, &SceneEntity), Changed<UiTransform>>,
    mut ui_roots: Query<&mut SceneUiData>,
) {
    for (ent, scene_id) in changed_entities.iter() {
        let Ok(mut ui_data) = ui_roots.get_mut(scene_id.root) else {
            warn!("scene root missing for {:?}", scene_id.root);
            continue;
        };

        ui_data.nodes.insert(ent);
        ui_data.relayout = true;
    }
}

#[derive(Component, Clone, PartialEq)]
pub struct UiLink {
    // the bevy ui entity corresponding to this scene entity
    ui_entity: Entity,
    // where child entities should be added
    content_entity: Entity,
    // opacity
    opacity: FloatOrd,
    // is this is part of the toplevel window ui
    is_window_ui: bool,
    // is scrollable
    scroll_entity: Option<Entity>,
    // current scroll target
    scroll_position: Option<ScrollPositionValue>,
}

impl Default for UiLink {
    fn default() -> Self {
        Self {
            ui_entity: Entity::PLACEHOLDER,
            content_entity: Entity::PLACEHOLDER,
            opacity: FloatOrd(1.0),
            is_window_ui: true,
            scroll_entity: None,
            scroll_position: None,
        }
    }
}

#[derive(Component)]
pub struct SceneUiRoot {
    scene: Entity,
    canvas: Entity,
}

fn create_ui_roots(
    mut commands: Commands,
    mut scene_uis: Query<(Entity, Option<&UiLink>), With<SceneUiData>>,
    player: Query<Entity, With<PrimaryUser>>,
    containing_scene: ContainingScene,
    current_uis: Query<(Entity, &SceneUiRoot)>,
    config: Res<AppConfig>,
    mut canvas_infos: Query<(
        Entity,
        &ContainerEntity,
        &UiCanvas,
        Option<&UiLink>,
        Option<&mut UiTextureOutput>,
    )>,
    images: ResMut<Assets<Image>>,
) {
    let images = images.into_inner();

    let current_scenes = player
        .get_single()
        .ok()
        .map(|p| containing_scene.get(p))
        .unwrap_or_default();

    // remove any non-current uis
    for (ent, ui_root) in &current_uis {
        if !current_scenes.contains(&ui_root.scene) {
            commands.entity(ent).despawn_recursive();
            if let Some(mut commands) = commands.get_entity(ui_root.canvas) {
                commands.remove::<UiLink>();
            }
        }
    }

    // spawn window root ui nodes
    for (ent, maybe_link) in scene_uis.iter_mut() {
        if current_scenes.contains(&ent) && (maybe_link.is_none() || config.is_changed()) {
            let root_style = if config.constrain_scene_ui {
                Style {
                    position_type: PositionType::Absolute,
                    left: Val::VMin(27.0),
                    right: Val::VMin(12.0),
                    top: Val::VMin(6.0),
                    bottom: Val::VMin(6.0),
                    overflow: Overflow::clip(),
                    ..Default::default()
                }
            } else {
                Style {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..Default::default()
                }
            };

            let window_root = commands
                .spawn((
                    NodeBundle {
                        style: root_style,
                        ..Default::default()
                    },
                    SceneUiRoot {
                        scene: ent,
                        canvas: ent,
                    },
                    DespawnWith(ent),
                ))
                .id();
            debug!("create window root {:?} -> {:?}", ent, window_root);
            commands.entity(ent).try_insert(UiLink {
                ui_entity: window_root,
                content_entity: window_root,
                ..Default::default()
            });
        }
    }

    // spawn texture ui nodes
    for (ent, container, UiCanvas(canvas_info), maybe_link, maybe_texture) in
        canvas_infos.iter_mut()
    {
        if current_scenes.contains(&container.root) {
            let ui_entity = match maybe_link {
                Some(link) => link.ui_entity,
                None => {
                    let existing_texture = maybe_texture.as_ref().map(|t| &t.image);
                    debug!("create w/existing {existing_texture:?}");
                    let (root, ui_texture) =
                        world_ui::spawn_world_ui_view(&mut commands, images, existing_texture);
                    commands.entity(root).try_insert((
                        TargetCamera(root),
                        ManualCursorPosition::default(),
                        SceneUiRoot {
                            scene: container.root,
                            canvas: ent,
                        },
                        DespawnWith(ent),
                        NodeBundle {
                            style: Style {
                                position_type: PositionType::Absolute,
                                width: Val::Percent(100.0),
                                height: Val::Percent(100.0),
                                ..Default::default()
                            },
                            z_index: ZIndex::Global(-2), // behind the ZIndex(-1) MouseInteractionComponent
                            ..Default::default()
                        },
                    ));
                    debug!("create texture root {:?} -> {:?}", ent, root);

                    images.get_mut(&ui_texture).unwrap().resize(Extent3d {
                        width: canvas_info.width,
                        height: canvas_info.height,
                        depth_or_array_layers: 1,
                    });

                    commands.entity(ent).try_insert(UiTextureOutput {
                        camera: root,
                        image: ui_texture,
                        texture_size: UVec2::new(canvas_info.width, canvas_info.height),
                    });

                    commands.entity(ent).try_insert(UiLink {
                        ui_entity: root,
                        is_window_ui: false,
                        content_entity: root,
                        ..Default::default()
                    });
                    root
                }
            };

            // update dimensions if required
            if let Some(mut texture) = maybe_texture {
                if canvas_info.width != texture.texture_size.x
                    || canvas_info.height != texture.texture_size.y
                {
                    images
                        .get_mut(texture.image.id())
                        .unwrap()
                        .resize(Extent3d {
                            width: canvas_info.width,
                            height: canvas_info.height,
                            depth_or_array_layers: 1,
                        });
                    texture.texture_size = UVec2::new(canvas_info.width, canvas_info.height);
                }
            }

            // and background
            let color = canvas_info.color.map(Into::into).unwrap_or(Color::NONE);
            commands
                .entity(ui_entity)
                .modify_component(move |c: &mut Camera| {
                    c.clear_color = bevy::render::camera::ClearColorConfig::Custom(color)
                });
        }
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn layout_scene_ui(
    mut commands: Commands,
    mut scene_uis: Query<(Entity, &mut SceneUiData)>,
    player: Query<Entity, With<PrimaryUser>>,
    containing_scene: ContainingScene,
    ui_nodes: Query<(&SceneEntity, Ref<UiTransform>, &Parent)>,
    config: Res<AppConfig>,
    mut removed_transforms: RemovedComponents<UiTransform>,
    ui_links: Query<&UiLink>,
    parents: Query<&Parent>,
    dui: Res<DuiRegistry>,
) {
    let current_scenes = player
        .get_single()
        .ok()
        .map(|p| containing_scene.get(p))
        .unwrap_or_default();

    let removed_transforms = removed_transforms.read().collect::<Vec<_>>();

    for (scene_root, mut ui_data) in scene_uis.iter_mut() {
        if !current_scenes.contains(&scene_root) {
            ui_data.relayout = true;
            continue;
        }

        let any_removed = removed_transforms.iter().any(|r| ui_data.nodes.contains(r));
        if !(ui_data.relayout || config.is_changed() || any_removed) {
            continue;
        }

        debug!(
            "redraw {:?} ui (removed: {}, relayout: {}, config changed: {})",
            scene_root,
            any_removed,
            ui_data.relayout,
            config.is_changed()
        );
        ui_data.relayout = false;

        // collect ui data
        let mut deleted_nodes = HashSet::default();
        let mut unprocessed_uis = BTreeMap::from_iter(ui_data.nodes.iter().flat_map(|node| {
            match ui_nodes.get(*node) {
                Ok((scene_entity, transform, bevy_parent)) => Some((
                    scene_entity.id,
                    (
                        *node,
                        transform.clone(),
                        transform.is_changed(),
                        bevy_parent.get(),
                    ),
                )),
                Err(_) => {
                    // remove this node
                    deleted_nodes.insert(*node);
                    None
                }
            }
        }));

        let mut valid_nodes = HashMap::new();
        let mut invalid_ui_entities = HashSet::new();
        let mut named_nodes = HashMap::new();
        let mut pending_scroll_events = HashMap::new();

        let mut modified = true;
        while modified && !unprocessed_uis.is_empty() {
            modified = false;
            unprocessed_uis.retain(
                |scene_id, (bevy_entity, ui_transform, transform_is_changed, root_node)| {
                    let Ok(bevy_ui_root) = ui_links.get(*root_node).cloned() else {
                        warn!("no root for {:?}", root_node);
                        return false;
                    };

                    // if our rightof is not added, we can't process this node
                    if ui_transform.right_of != SceneEntityId::ROOT
                        && !valid_nodes.contains_key(&ui_transform.right_of)
                    {
                        return true;
                    }

                    // if our parent is not added, we can't process this node
                    let parent = if ui_transform.parent == SceneEntityId::ROOT {
                        Some(&bevy_ui_root)
                    } else {
                        valid_nodes.get(&ui_transform.parent)
                    };

                    let Some(parent_link) = parent else {
                        return true;
                    };

                    // get or create the counterpart ui entity
                    let existing_link = if let Ok(link) = ui_links.get(*bevy_entity) {
                        if commands.get_entity(link.ui_entity).is_none() {
                            None
                        } else if link.scroll_entity.is_some() == ui_transform.scroll {
                            debug!("{scene_id} reuse linked {:?}", link.ui_entity);
                            Some(link)
                        } else {
                            // queue to despawn
                            invalid_ui_entities.insert(link.ui_entity);
                            None
                        }
                    } else {
                        None
                    };

                    let existing = if let Some(link) = existing_link {
                        // update parent if required
                        if parents.get(link.ui_entity).map(Parent::get)
                            != Ok(parent_link.content_entity)
                        {
                            commands
                                .entity(link.ui_entity)
                                .set_parent(parent_link.content_entity);
                        }
                        let updated = UiLink {
                            opacity: FloatOrd(parent_link.opacity.0 * ui_transform.opacity),
                            is_window_ui: bevy_ui_root.is_window_ui,
                            ..link.clone()
                        };
                        if &updated != link {
                            let updated = updated.clone();
                            commands
                                .entity(*bevy_entity)
                                .modify_component(move |link: &mut UiLink| *link = updated);
                        }
                        valid_nodes.insert(*scene_id, updated);
                        true
                    } else {
                        // we use entity id as zindex. this is rubbish but mimics the foundation behaviour for multiple overlapping root nodes.
                        let mut ent_cmds = commands.spawn(NodeBundle {
                            z_index: ZIndex::Local(scene_id.id as i32),
                            ..Default::default()
                        });
                        ent_cmds.set_parent(parent_link.content_entity);
                        let ui_entity = ent_cmds.id();
                        debug!("{scene_id} create linked {:?}", ui_entity);

                        let (scroll_entity, content_entity) = if ui_transform.scroll {
                            ent_cmds.try_insert(FocusPolicy::Block);
                            let content = ent_cmds.commands().spawn(NodeBundle::default()).id();
                            let scrollable = ent_cmds
                                .spawn_template(
                                    &dui,
                                    "scrollable-base",
                                    DuiProps::new()
                                        .with_prop(
                                            "scroll-settings",
                                            Scrollable::new()
                                                .with_direction(ScrollDirection::Both(
                                                    StartPosition::Explicit(0.0),
                                                    StartPosition::Explicit(0.0),
                                                ))
                                                .with_drag(true)
                                                .with_wheel(true)
                                                .with_bars_visible(
                                                    ui_transform.scroll_h_visible,
                                                    ui_transform.scroll_v_visible,
                                                ),
                                        )
                                        .with_prop("content", content),
                                )
                                .unwrap()
                                .root;

                            let scene_id = *scene_id;
                            ent_cmds
                            .commands()
                            .entity(scrollable)
                            .set_parent(ui_entity)
                            .try_insert(On::<DataChanged>::new(
                                move |caller: Res<UiCaller>,
                                    position: Query<&ScrollPosition>,
                                    mut context: Query<&mut RendererSceneContext>| {
                                    let Ok(pos) = position.get(caller.0) else {
                                        warn!("failed to get scroll pos on scrollable update");
                                        return;
                                    };
                                    let Ok(mut context) = context.get_mut(scene_root) else {
                                        warn!("failed to get context on scrollable update");
                                        return;
                                    };

                                    context.update_crdt(
                                        SceneComponentId::UI_SCROLL_RESULT,
                                        CrdtType::LWW_ENT,
                                        scene_id,
                                        &PbUiScrollResult {
                                            value: Some(Vec2::new(pos.h, pos.v).into()),
                                        },
                                    );
                                },
                            ));

                            (Some(scrollable), content)
                        } else {
                            (None, ui_entity)
                        };

                        let new_link = UiLink {
                            ui_entity,
                            is_window_ui: bevy_ui_root.is_window_ui,
                            content_entity,
                            scroll_entity,
                            opacity: FloatOrd(parent_link.opacity.0 * ui_transform.opacity),
                            scroll_position: None,
                        };
                        commands.entity(*bevy_entity).try_insert(new_link.clone());
                        valid_nodes.insert(*scene_id, new_link);
                        false
                    };

                    let link = valid_nodes.get(scene_id).unwrap();

                    // update style
                    if !existing || *transform_is_changed {
                        let style = Style {
                            align_content: ui_transform.align_content,
                            align_items: ui_transform.align_items,
                            flex_wrap: ui_transform.wrap,
                            position_type: ui_transform.position_type,
                            flex_shrink: ui_transform.shrink,
                            align_self: ui_transform.align_self,
                            flex_direction: ui_transform.flex_direction,
                            justify_content: ui_transform.justify_content,
                            overflow: ui_transform.overflow,
                            display: ui_transform.display,
                            flex_basis: ui_transform.basis,
                            flex_grow: ui_transform.grow,
                            width: ui_transform.size.width.unwrap_or_default(),
                            height: ui_transform.size.height.unwrap_or_default(),
                            min_width: ui_transform.min_size.width,
                            min_height: ui_transform.min_size.height,
                            max_width: ui_transform.max_size.width,
                            max_height: ui_transform.max_size.height,
                            left: ui_transform.position.left,
                            right: ui_transform.position.right,
                            top: ui_transform.position.top,
                            bottom: ui_transform.position.bottom,
                            margin: ui_transform.margin,
                            padding: ui_transform.padding,
                            ..Default::default()
                        };

                        // update inner style
                        if link.content_entity != link.ui_entity {
                            let new_style = style.clone();
                            commands.entity(link.content_entity).modify_component(
                                move |style: &mut Style| {
                                    style.align_content = new_style.align_content;
                                    style.align_items = new_style.align_items;
                                    style.flex_wrap = new_style.flex_wrap;
                                    style.flex_direction = new_style.flex_direction;
                                    style.justify_content = new_style.justify_content;
                                    style.overflow = new_style.overflow;
                                },
                            );
                        }

                        commands.entity(link.ui_entity).try_insert(style);
                    }

                    // gather scroll events
                    if let Some(scroll_entity) = link.scroll_entity {
                        if ui_transform.scroll_position != link.scroll_position {
                            // deferred update
                            let pos = ui_transform.scroll_position.clone();
                            commands.entity(*bevy_entity).modify_component(
                                move |link: &mut UiLink| {
                                    link.scroll_position = pos;
                                },
                            );

                            if let Some(ScrollPositionValue {
                                value: Some(ref target),
                            }) = ui_transform.scroll_position
                            {
                                match target {
                                    scroll_position_value::Value::Position(vec) => {
                                        debug!("scroll literal {vec:?}");
                                        commands.fire_event(ScrollTargetEvent {
                                            scrollable: scroll_entity,
                                            position: ScrollTarget::Literal(Vec2::from(vec)),
                                        });
                                    }
                                    scroll_position_value::Value::Reference(target) => {
                                        debug!("scroll target {target}");
                                        pending_scroll_events.insert(scroll_entity, target.clone());
                                    }
                                }
                            }
                        }
                    }

                    if let Some(name) = ui_transform.element_id.clone() {
                        named_nodes.insert(name, link.ui_entity);
                    }

                    // mark to continue and remove from unprocessed
                    modified = true;
                    false
                },
            );
        }

        debug!(
            "made ui; placed: {}, unplaced: {} ({:?})",
            valid_nodes.len(),
            unprocessed_uis.len(),
            unprocessed_uis
        );
        ui_data.relayout = false;

        // remove any dead nodes
        for node in deleted_nodes {
            if let Ok(link) = ui_links.get(node) {
                if let Some(commands) = commands.get_entity(link.ui_entity) {
                    debug!("{node} delete linked {:?}", link.ui_entity);
                    commands.despawn_recursive();
                }
            }
            ui_data.nodes.remove(&node);
        }

        // and any unused linked entities
        for (_, (node, ..)) in unprocessed_uis {
            if let Ok(link) = ui_links.get(node) {
                if let Some(commands) = commands.get_entity(link.ui_entity) {
                    debug!("{node} delete linked {:?}", link.ui_entity);
                    commands.despawn_recursive();
                }
            }
        }

        // and any invalidated nodes
        for node in invalid_ui_entities {
            debug!("?? delete linked {:?}", node);
            commands.entity(node).despawn_recursive();
        }

        // send any pending events
        for (scrollable, target) in pending_scroll_events {
            if let Some(target) = named_nodes.get(&target) {
                commands.fire_event(ScrollTargetEvent {
                    scrollable,
                    position: ScrollTarget::Entity(*target),
                });
            } else {
                warn!("scroll to target `{target}` not found");
            }
        }
    }
}

fn set_ui_input(
    mut commands: Commands,
    inputs: Query<(&SceneEntity, &UiInput, &UiLink), Or<(Changed<UiInput>, Changed<UiLink>)>>,
    mut removed: RemovedComponents<UiInput>,
    links: Query<&UiLink>,
) {
    for ent in removed.read() {
        if let Ok(link) = links.get(ent) {
            if let Some(mut commands) = commands.get_entity(link.ui_entity) {
                commands.remove::<TextEntry>();
            }
        }
    }

    for (scene_ent, input, link) in inputs.iter() {
        let Some(mut commands) = commands.get_entity(link.ui_entity) else {
            continue;
        };

        let font_size = input.0.font_size.unwrap_or(12);
        let ui_entity = link.ui_entity;
        let root = scene_ent.root;
        let scene_id = scene_ent.id;

        let data_handler = move |In(submit): In<bool>,
                                 entry: Query<&TextEntry>,
                                 mut context: Query<&mut RendererSceneContext>,
                                 time: Res<Time>,
                                 caller: Res<UiCaller>| {
            debug!("callback on {:?}", caller.0);
            let Ok(entry) = entry.get(ui_entity) else {
                warn!("failed to get text node on UiInput update");
                return;
            };
            let Ok(mut context) = context.get_mut(root) else {
                warn!("failed to get context on UiInput update");
                return;
            };

            context.update_crdt(
                SceneComponentId::UI_INPUT_RESULT,
                CrdtType::LWW_ENT,
                scene_id,
                &PbUiInputResult {
                    value: entry.content.clone(),
                    is_submit: Some(submit),
                },
            );
            context.last_action_event = Some(time.elapsed_seconds());
        };

        commands.modify_component(move |style: &mut Style| {
            //ensure we use max width if not given
            if style.width == Val::Px(0.0) {
                style.width = Val::Percent(100.0);
            }
            //and some size if not given
            if style.height == Val::Px(0.0) {
                style.height = Val::Px(font_size as f32 * 1.3);
            }
        });

        commands.try_insert((
            FocusPolicy::Block,
            Interaction::default(),
            TextEntry {
                hint_text: input.0.placeholder.to_owned(),
                enabled: !input.0.disabled,
                content: input.0.value.clone().unwrap_or_default(),
                accept_line: false,
                font_size,
                ..Default::default()
            },
            On::<DataChanged>::new((|| false).pipe(data_handler)),
            On::<Submit>::new((|| true).pipe(data_handler)),
        ));
    }
}

fn set_ui_dropdown(
    mut commands: Commands,
    dropdowns: Query<
        (&SceneEntity, &UiDropdown, &UiLink),
        Or<(Changed<UiDropdown>, Changed<UiLink>)>,
    >,
    mut removed: RemovedComponents<UiDropdown>,
    links: Query<&UiLink>,
) {
    for ent in removed.read() {
        if let Ok(link) = links.get(ent) {
            if let Some(mut commands) = commands.get_entity(link.ui_entity) {
                commands.remove::<ComboBox>();
            }
        }
    }

    for (scene_ent, dropdown, link) in dropdowns.iter() {
        let Some(mut commands) = commands.get_entity(link.ui_entity) else {
            continue;
        };

        let initial_selection = if dropdown.0.accept_empty {
            dropdown.0.selected_index.map(|ix| ix as isize)
        } else {
            Some(dropdown.0.selected_index.unwrap_or(0) as isize)
        };

        commands.modify_component(|style: &mut Style| {
            //ensure we use max width if not given
            if style.width == Val::Px(0.0) || style.width == Val::Auto {
                style.width = Val::Percent(100.0);
            }
            //and some size if not given
            if style.height == Val::Px(0.0) || style.height == Val::Auto {
                style.height = Val::Px(16.0);
            }
        });

        let root = scene_ent.root;
        let ui_entity = link.ui_entity;
        let scene_id = scene_ent.id;
        commands.try_insert((
            ComboBox::new(
                dropdown.0.empty_label.clone().unwrap_or_default(),
                &dropdown.0.options,
                dropdown.0.accept_empty,
                dropdown.0.disabled,
                initial_selection,
            ),
            On::<DataChanged>::new(
                move |combo: Query<(Entity, &ComboBox)>,
                      mut context: Query<&mut RendererSceneContext>,
                      time: Res<Time>| {
                    let Ok((_, combo)) = combo.get(ui_entity) else {
                        warn!("failed to get combo node on UiDropdown update");
                        return;
                    };
                    let Ok(mut context) = context.get_mut(root) else {
                        warn!("failed to get context on UiInput update");
                        return;
                    };

                    context.update_crdt(
                        SceneComponentId::UI_DROPDOWN_RESULT,
                        CrdtType::LWW_ENT,
                        scene_id,
                        &PbUiDropdownResult {
                            value: combo.selected as i32,
                        },
                    );
                    context.last_action_event = Some(time.elapsed_seconds());
                },
            ),
        ));
    }
}

fn set_ui_pointer_events(
    mut commands: Commands,
    pes: Query<
        (Entity, &UiLink),
        (
            With<PointerEvents>,
            Or<(Changed<PointerEvents>, Changed<UiLink>)>,
        ),
    >,
) {
    for (ent, link) in pes.iter() {
        if let Some(mut commands) = commands.get_entity(link.ui_entity) {
            let is_primary = link.is_window_ui;
            commands.try_insert((
                FocusPolicy::Block,
                Interaction::default(),
                On::<HoverEnter>::new(move |mut ui_target: ResMut<UiPointerTarget>| {
                    if is_primary {
                        *ui_target = UiPointerTarget::Primary(ent);
                    } else {
                        *ui_target = UiPointerTarget::World(ent);
                    }
                }),
                On::<HoverExit>::new(move |mut ui_target: ResMut<UiPointerTarget>| {
                    if *ui_target == UiPointerTarget::Primary(ent)
                        || *ui_target == UiPointerTarget::World(ent)
                    {
                        *ui_target = UiPointerTarget::None;
                    };
                }),
            ));
        }
    }
}

#[derive(Component)]
pub struct UiTextMarker;

fn set_ui_text(
    mut commands: Commands,
    texts: Query<(&UiText, &UiTransform, &UiLink), Or<(Changed<UiText>, Changed<UiLink>)>>,
    mut removed: RemovedComponents<UiText>,
    links: Query<&UiLink>,
    children: Query<&Children>,
    prev_texts: Query<&UiTextMarker>,
) {
    for ent in removed.read() {
        let Ok(link) = links.get(ent) else {
            continue;
        };

        if let Ok(children) = children.get(link.ui_entity) {
            for child in children.iter().filter(|c| prev_texts.get(**c).is_ok()) {
                if let Some(commands) = commands.get_entity(*child) {
                    commands.despawn_recursive();
                }
            }
        }
    }

    for (ui_text, ui_transform, link) in texts.iter() {
        // remove old text
        if let Ok(children) = children.get(link.ui_entity) {
            for child in children.iter().filter(|c| prev_texts.get(**c).is_ok()) {
                if let Some(commands) = commands.get_entity(*child) {
                    commands.despawn_recursive();
                }
            }
        }

        if ui_text.text.is_empty() || ui_text.font_size <= 0.0 {
            continue;
        }

        let Some(mut ent_cmds) = commands.get_entity(link.ui_entity) else {
            continue;
        };

        let text = make_text_section(
            ui_text.text.as_str(),
            ui_text.font_size,
            ui_text
                .color
                .with_alpha(ui_text.color.alpha() * link.opacity.0),
            ui_text.font,
            ui_text.h_align,
            ui_text.wrapping,
        );

        // with text nodes the axis sizes are unusual.
        // a) if either size axis is NOT NONE, (explicit or auto), we want auto to size appropriately for the content.
        // b) if both axes are NONE, we want to size to zero.
        // a) - we tackle this by using a nested position-type: relative node which will size it's parent appropriately, and default the parent to Auto
        //    - for alignment we use align-items and justify-content
        // b) - we use a nested position-type: absolute node, and default the parent to auto
        //    - for alignment we use align-items and justify-content as above, and we also set left/right/top/bottom to 50% if required

        let any_axis_specified = [ui_transform.size.width, ui_transform.size.height]
            .iter()
            .any(Option::is_some);

        let inner_style = if any_axis_specified {
            Style {
                position_type: PositionType::Relative,
                ..Default::default()
            }
        } else {
            Style {
                position_type: PositionType::Absolute,
                left: if ui_text.h_align == JustifyText::Left {
                    Val::Percent(50.0)
                } else {
                    Val::Auto
                },
                right: if ui_text.h_align == JustifyText::Right {
                    Val::Percent(50.0)
                } else {
                    Val::Auto
                },
                top: if ui_text.v_align == VAlign::Top {
                    Val::Percent(50.0)
                } else {
                    Val::Auto
                },
                bottom: if ui_text.v_align == VAlign::Bottom {
                    Val::Percent(50.0)
                } else {
                    Val::Auto
                },
                ..Default::default()
            }
        };

        // we need to set size for the first inner element depending
        // on how the outer was specified
        let width = match ui_transform.size.width {
            Some(Val::Px(px)) => Val::Px(px),
            Some(Val::Percent(_)) => Val::Percent(100.0),
            _ => Val::Auto,
        };
        let height = match ui_transform.size.height {
            Some(Val::Px(px)) => Val::Px(px),
            Some(Val::Percent(_)) => Val::Percent(100.0),
            _ => Val::Auto,
        };

        ent_cmds.try_with_children(|c| {
            c.spawn((
                NodeBundle {
                    style: Style {
                        flex_direction: FlexDirection::Row,
                        justify_content: match ui_text.h_align {
                            JustifyText::Left => JustifyContent::FlexStart,
                            JustifyText::Center => JustifyContent::Center,
                            JustifyText::Right => JustifyContent::FlexEnd,
                            JustifyText::Justified => unreachable!(),
                        },
                        align_items: match ui_text.v_align {
                            VAlign::Top => AlignItems::FlexStart,
                            VAlign::Middle => AlignItems::Center,
                            VAlign::Bottom => AlignItems::FlexEnd,
                        },
                        width,
                        height,
                        align_self: AlignSelf::FlexStart,
                        // elements are horizontally centered by default
                        margin: UiRect::horizontal(Val::Auto),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                UiTextMarker,
            ))
            .try_with_children(|c| {
                c.spawn(NodeBundle {
                    style: inner_style,
                    ..Default::default()
                })
                .try_with_children(|c| {
                    c.spawn(TextBundle {
                        text,
                        z_index: ZIndex::Local(1),
                        ..Default::default()
                    });
                });
            });
        });
    }
}

#[derive(Component)]
pub struct UiBackgroundMarker;

fn set_ui_background(
    mut commands: Commands,
    backgrounds: Query<
        (&SceneEntity, &UiBackground, &UiLink),
        Or<(Changed<UiBackground>, Changed<UiLink>)>,
    >,
    mut removed: RemovedComponents<UiBackground>,
    links: Query<&UiLink>,
    children: Query<&Children>,
    prev_backgrounds: Query<Entity, With<UiBackgroundMarker>>,
    contexts: Query<&RendererSceneContext>,
    resolver: TextureResolver,
    mut stretch_uvs: ResMut<Assets<StretchUvMaterial>>,
) {
    for ent in removed.read() {
        let Ok(link) = links.get(ent) else {
            continue;
        };

        if let Ok(children) = children.get(link.ui_entity) {
            for child in children
                .iter()
                .filter(|c| prev_backgrounds.get(**c).is_ok())
            {
                if let Some(commands) = commands.get_entity(*child) {
                    commands.despawn_recursive();
                }
            }
        }

        if let Some(mut commands) = commands.get_entity(link.ui_entity) {
            commands.remove::<BackgroundColor>();
        }
    }

    for (scene_ent, background, link) in backgrounds.iter() {
        let Some(mut commands) = commands.get_entity(link.ui_entity) else {
            continue;
        };

        debug!("[{}] set background {:?}", scene_ent.id, background);

        if let Some(texture) = background.texture.as_ref() {
            let Ok(ctx) = contexts.get(scene_ent.root) else {
                continue;
            };

            let image = texture
                .tex
                .tex
                .as_ref()
                .and_then(|tex| resolver.resolve_texture(ctx, tex).ok());

            let texture_mode = match texture.tex.tex {
                Some(texture_union::Tex::Texture(_)) => texture.mode,
                _ => BackgroundTextureMode::stretch_default(),
            };

            if let Some(image) = image {
                let image_color = background.color.unwrap_or(Color::WHITE);
                let image_color = image_color.with_alpha(image_color.alpha() * link.opacity.0);
                match texture_mode {
                    BackgroundTextureMode::NineSlices(rect) => {
                        commands.try_with_children(|c| {
                            c.spawn((
                                NodeBundle {
                                    style: Style {
                                        position_type: PositionType::Absolute,
                                        width: Val::Percent(100.0),
                                        height: Val::Percent(100.0),
                                        overflow: Overflow::clip(),
                                        ..Default::default()
                                    },
                                    ..Default::default()
                                },
                                Ui9Slice {
                                    image: image.image,
                                    center_region: rect.into(),
                                    tint: Some(image_color),
                                },
                            ));
                        });
                    }
                    BackgroundTextureMode::Stretch(ref uvs) => {
                        commands.try_with_children(|c| {
                            c.spawn(NodeBundle {
                                style: Style {
                                    position_type: PositionType::Absolute,
                                    width: Val::Percent(100.0),
                                    height: Val::Percent(100.0),
                                    overflow: Overflow::clip(),
                                    ..Default::default()
                                },
                                ..Default::default()
                            })
                            .try_with_children(|c| {
                                c.spawn((MaterialNodeBundle {
                                    style: Style {
                                        position_type: PositionType::Absolute,
                                        width: Val::Percent(100.0),
                                        height: Val::Percent(100.0),
                                        ..Default::default()
                                    },
                                    material: stretch_uvs.add(StretchUvMaterial {
                                        image: image.image.clone(),
                                        uvs: *uvs,
                                        color: image_color.to_linear().to_vec4(),
                                    }),
                                    ..Default::default()
                                },));
                            });
                        });
                    }
                    BackgroundTextureMode::Center => {
                        commands.try_with_children(|c| {
                            // make a stretchy grid
                            c.spawn(NodeBundle {
                                style: Style {
                                    position_type: PositionType::Absolute,
                                    left: Val::Px(0.0),
                                    right: Val::Px(0.0),
                                    top: Val::Px(0.0),
                                    bottom: Val::Px(0.0),
                                    justify_content: JustifyContent::Center,
                                    overflow: Overflow::clip(),
                                    width: Val::Percent(100.0),
                                    ..Default::default()
                                },
                                ..Default::default()
                            })
                            .try_with_children(|c| {
                                c.spacer();
                                c.spawn(NodeBundle {
                                    style: Style {
                                        flex_direction: FlexDirection::Column,
                                        justify_content: JustifyContent::Center,
                                        overflow: Overflow::clip(),
                                        height: Val::Percent(100.0),
                                        ..Default::default()
                                    },
                                    ..Default::default()
                                })
                                .try_with_children(|c| {
                                    c.spacer();
                                    c.spawn(ImageBundle {
                                        style: Style {
                                            overflow: Overflow::clip(),
                                            ..Default::default()
                                        },
                                        image: UiImage {
                                            color: image_color,
                                            texture: image.image,
                                            flip_x: false,
                                            flip_y: false,
                                        },
                                        ..Default::default()
                                    });
                                    c.spacer();
                                });
                                c.spacer();
                            });
                        });
                    }
                }
            } else {
                warn!("failed to load ui image from content map: {:?}", texture);
            }
        } else if let Some(color) = background.color {
            commands.insert(BackgroundColor(color));
        }
    }
}

pub trait ValAsPx {
    fn as_px(&self) -> f32;
}

impl ValAsPx for Val {
    fn as_px(&self) -> f32 {
        match self {
            Val::Px(px) => *px,
            _ => 0.0,
        }
    }
}

pub fn fully_update_target_camera_system(
    mut commands: Commands,
    root_nodes_query: Query<(Entity, Option<&TargetCamera>), (With<Node>, Without<Parent>)>,
    children_query: Query<&Children, With<Node>>,
) {
    // Track updated entities to prevent redundant updates, as `Commands` changes are deferred,
    // and updates done for changed_children_query can overlap with itself or with root_node_query
    let mut updated_entities = HashSet::new();

    for (root_node, target_camera) in &root_nodes_query {
        update_children_target_camera(
            root_node,
            target_camera,
            &children_query,
            &mut commands,
            &mut updated_entities,
        );
    }
}

fn update_children_target_camera(
    entity: Entity,
    camera_to_set: Option<&TargetCamera>,
    children_query: &Query<&Children, With<Node>>,
    commands: &mut Commands,
    updated_entities: &mut HashSet<Entity>,
) {
    let Ok(children) = children_query.get(entity) else {
        return;
    };

    for &child in children {
        // Skip if the child has already been updated
        if updated_entities.contains(&child) {
            continue;
        }

        match camera_to_set {
            Some(camera) => {
                commands.entity(child).try_insert(camera.clone());
            }
            None => {
                commands.entity(child).remove::<TargetCamera>();
            }
        }
        updated_entities.insert(child);

        update_children_target_camera(
            child,
            camera_to_set,
            children_query,
            commands,
            updated_entities,
        );
    }
}
