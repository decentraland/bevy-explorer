pub mod ui_background;
pub mod ui_dropdown;
pub mod ui_input;
pub mod ui_pointer;
pub mod ui_text;

use std::collections::{BTreeSet, VecDeque};

use bevy::{
    math::FloatOrd,
    prelude::*,
    render::render_resource::Extent3d,
    ui::{FocusPolicy, ManualCursorPosition},
    utils::{HashMap, HashSet},
};
use bevy_console::ConsoleCommand;
use bevy_dui::{DuiCommandsExt, DuiProps, DuiRegistry};
use console::DoAddConsoleCommand;
use ui_background::{set_ui_background, UiBackground};
use ui_dropdown::{set_ui_dropdown, UiDropdown};
use ui_input::{set_ui_input, UiInput};
use ui_pointer::set_ui_pointer_events;
use ui_text::{set_ui_text, UiText};

use crate::{
    initialize_scene::{LiveScenes, SuperUserScene},
    renderer_context::RendererSceneContext,
    ContainerEntity, ContainingScene, SceneEntity, SceneSets,
};
use common::{
    structs::{AppConfig, PrimaryUser},
    util::{DespawnWith, FireEventEx, ModifyComponentExt},
};
use dcl::interface::{ComponentPosition, CrdtType};
use dcl_component::{
    proto_components::{
        sdk::components::{
            self, scroll_position_value, PbUiBackground, PbUiCanvas, PbUiDropdown, PbUiInput,
            PbUiScrollResult, PbUiText, PbUiTransform, ScrollPositionValue, YgAlign, YgDisplay,
            YgFlexDirection, YgJustify, YgOverflow, YgPositionType, YgUnit, YgWrap,
        },
        Color4DclToBevy,
    },
    SceneComponentId, SceneEntityId,
};
use ui_core::{
    scrollable::{
        ScrollDirection, ScrollPosition, ScrollTarget, ScrollTargetEvent, Scrollable, StartPosition,
    },
    ui_actions::{DataChanged, On, UiCaller},
};

use super::AddCrdtInterfaceExt;

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

trait ValueOrDefault {
    type Value;
    fn value_or_default(&self) -> Self::Value;
}

impl ValueOrDefault for f32 {
    type Value = f32;
    fn value_or_default(&self) -> Self::Value {
        *self
    }
}

impl ValueOrDefault for Option<f32> {
    type Value = f32;
    fn value_or_default(&self) -> Self::Value {
        self.unwrap_or_default()
    }
}

// macro helpers to convert proto format to bevy format for val, size, rect
macro_rules! val {
    ($pb:ident, $u:ident, $v:ident, $d:expr) => {
        match $pb.$u() {
            YgUnit::YguUndefined => $d,
            YgUnit::YguAuto => Val::Auto,
            YgUnit::YguPoint => {
                if $pb.$v.value_or_default().is_nan() {
                    $d
                } else {
                    Val::Px($pb.$v.value_or_default())
                }
            }
            YgUnit::YguPercent => {
                if $pb.$v.value_or_default().is_nan() {
                    $d
                } else {
                    Val::Percent($pb.$v.value_or_default())
                }
            }
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
            YgUnit::YguPoint => Some(if $pb.$v.is_nan() { $d } else { Val::Px($pb.$v) }),
            YgUnit::YguPercent => Some(if $pb.$v.is_nan() {
                $d
            } else {
                Val::Percent($pb.$v)
            }),
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

macro_rules! radius {
    ($pb:ident, $tlu:ident, $tl:ident, $tru:ident, $tr:ident, $blu:ident, $bl:ident, $bru:ident, $br:ident, $d:expr) => {
        BorderRadius {
            top_left: val!($pb, $tlu, $tl, $d),
            top_right: val!($pb, $tru, $tr, $d),
            bottom_left: val!($pb, $blu, $bl, $d),
            bottom_right: val!($pb, $bru, $br, $d),
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
    zindex: Option<i16>,
    border: UiRect,
    border_radius: BorderRadius,
    border_color: BorderColor,
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
            zindex: value.z_index.map(|z| z as i16),
            border: rect!(
                value,
                border_left_width_unit,
                border_left_width,
                border_right_width_unit,
                border_right_width,
                border_top_width_unit,
                border_top_width,
                border_bottom_width_unit,
                border_bottom_width,
                Val::Auto
            ),
            border_radius: radius!(
                value,
                border_top_left_radius_unit,
                border_top_left_radius,
                border_top_right_radius_unit,
                border_top_right_radius,
                border_bottom_left_radius_unit,
                border_bottom_left_radius,
                border_bottom_right_radius_unit,
                border_bottom_right_radius,
                Val::ZERO
            ),
            border_color: BorderColor {
                top: value
                    .border_top_color
                    .map(Color4DclToBevy::convert_srgba)
                    .unwrap_or(Color::NONE),
                bottom: value
                    .border_bottom_color
                    .map(Color4DclToBevy::convert_srgba)
                    .unwrap_or(Color::NONE),
                left: value
                    .border_left_color
                    .map(Color4DclToBevy::convert_srgba)
                    .unwrap_or(Color::NONE),
                right: value
                    .border_right_color
                    .map(Color4DclToBevy::convert_srgba)
                    .unwrap_or(Color::NONE),
            },
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

        app.init_resource::<HiddenSceneUis>();

        app.add_systems(Update, init_scene_ui_root.in_set(SceneSets::PostInit));
        app.add_systems(
            Update,
            (
                update_scene_ui_components,
                create_ui_roots,
                layout_scene_ui,
                (
                    set_ui_text,       // text runs before background as both insert to position 0.
                    set_ui_background, // so text is actually in front of background, but "behind"/before children
                    set_ui_input,
                    set_ui_dropdown,
                    set_ui_pointer_events,
                ),
                fully_update_target_camera_system,
            )
                .chain()
                .in_set(SceneSets::PostLoop),
        );
        app.add_console_command::<ToggleSceneUiCommand, _>(toggle_scene_ui_command);
    }
}

#[derive(Component, Default)]
pub struct SceneUiData {
    nodes: BTreeSet<Entity>,
    relayout: bool,
    super_user: bool,
}

#[derive(Component)]
pub struct UiTextureOutput {
    pub camera: Entity,
    pub image: Handle<Image>,
    pub texture_size: UVec2,
}

fn init_scene_ui_root(
    mut commands: Commands,
    scenes: Query<
        (Entity, Has<SuperUserScene>),
        (With<RendererSceneContext>, Without<SceneUiData>),
    >,
) {
    for (scene_ent, super_user) in scenes.iter() {
        commands.entity(scene_ent).try_insert(SceneUiData {
            super_user,
            ..Default::default()
        });
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
    pub ui_entity: Entity,
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
    mut scene_uis: Query<(
        Entity,
        &RendererSceneContext,
        Option<&UiLink>,
        &SceneUiData,
        Option<&SuperUserScene>,
    )>,
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
    hidden_uis: Res<HiddenSceneUis>,
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
    for (ent, context, maybe_link, ui_data, maybe_super) in scene_uis.iter_mut() {
        if current_scenes.contains(&ent) && (maybe_link.is_none() || config.is_changed()) {
            let display = if maybe_super.is_some()
                || hidden_uis
                    .scenes
                    .get(&context.hash)
                    .copied()
                    .unwrap_or(hidden_uis.show_all)
            {
                Display::Flex
            } else {
                Display::None
            };

            let root_style = if config.constrain_scene_ui {
                Style {
                    display,
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
                    display,
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..Default::default()
                }
            };

            let z_index = ZIndex::Global(if ui_data.super_user { 1 << 17 } else { 0 });

            let window_root = commands
                .spawn((
                    NodeBundle {
                        style: root_style,
                        z_index,
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
                            z_index: ZIndex::Global(0), // behind the ZIndex((1 << 18) + 1) MouseInteractionComponent
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
            let color = canvas_info
                .color
                .map(Color4DclToBevy::convert_srgba)
                .unwrap_or(Color::NONE);
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
        let mut unprocessed_uis = ui_data
            .nodes
            .iter()
            .flat_map(|node| {
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
            })
            .collect::<Vec<_>>();
        unprocessed_uis.sort_by_key(|(scene_id, _)| *scene_id);
        let mut unprocessed_uis: VecDeque<_> = unprocessed_uis.into();

        let mut valid_nodes = HashMap::new();
        let mut invalid_ui_entities = HashSet::new();
        let mut named_nodes = HashMap::new();
        let mut pending_scroll_events = HashMap::new();

        let mut blocked_elements: HashMap<
            SceneEntityId,
            Vec<(SceneEntityId, (Entity, UiTransform, bool, Entity))>,
        > = HashMap::default();

        while let Some((scene_id, (bevy_entity, ui_transform, transform_is_changed, root_node))) =
            unprocessed_uis.pop_front()
        {
            let Ok(bevy_ui_root) = ui_links.get(root_node).cloned() else {
                warn!("no root for {:?}", root_node);
                continue;
            };

            // if our rightof is not added, we can't process this node
            if ui_transform.right_of != SceneEntityId::ROOT
                && !valid_nodes.contains_key(&ui_transform.right_of)
            {
                blocked_elements
                    .entry(ui_transform.right_of)
                    .or_default()
                    .push((
                        scene_id,
                        (bevy_entity, ui_transform, transform_is_changed, root_node),
                    ));
                continue;
            }

            // if our parent is not added, we can't process this node
            let parent = if ui_transform.parent == SceneEntityId::ROOT {
                Some(&bevy_ui_root)
            } else {
                valid_nodes.get(&ui_transform.parent)
            };

            let Some(parent_link) = parent else {
                blocked_elements
                    .entry(ui_transform.parent)
                    .or_default()
                    .push((
                        scene_id,
                        (bevy_entity, ui_transform, transform_is_changed, root_node),
                    ));
                continue;
            };

            // get or create the counterpart ui entity
            let existing_link = if let Ok(link) = ui_links.get(bevy_entity) {
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
                // update parent (always, so the child order is correct)
                commands
                    .entity(link.ui_entity)
                    .remove_parent()
                    .set_parent(parent_link.content_entity);
                let updated = UiLink {
                    opacity: FloatOrd(parent_link.opacity.0 * ui_transform.opacity),
                    is_window_ui: bevy_ui_root.is_window_ui,
                    ..link.clone()
                };
                if &updated != link {
                    let updated = updated.clone();
                    commands
                        .entity(bevy_entity)
                        .modify_component(move |link: &mut UiLink| *link = updated);
                }
                valid_nodes.insert(scene_id, updated);
                true
            } else {
                let mut ent_cmds =
                    commands.spawn((NodeBundle::default(), DespawnWith(bevy_entity)));
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
                commands.entity(bevy_entity).try_insert(new_link.clone());
                valid_nodes.insert(scene_id, new_link);
                false
            };

            let link = valid_nodes.get(&scene_id).unwrap();

            // update style
            if !existing || transform_is_changed {
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
                    border: ui_transform.border,
                    ..Default::default()
                };

                debug!("{scene_id} set style {ui_transform:?} -> {style:?}");

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

                let mut cmds = commands.entity(link.ui_entity);
                cmds.try_insert(style);

                if ui_transform.border_radius != BorderRadius::DEFAULT {
                    cmds.try_insert(ui_transform.border_radius);
                } else {
                    cmds.remove::<BorderRadius>();
                }

                if ui_transform.border_color != BorderColor::DEFAULT {
                    cmds.try_insert(ui_transform.border_color);
                } else {
                    cmds.remove::<BorderColor>();
                }

                let mut zindex_added = false;
                if let Some(zindex) = ui_transform.zindex {
                    if zindex != 0 {
                        zindex_added = true;
                        cmds.try_insert(ZIndex::Global(
                            zindex as i32 + if ui_data.super_user { 1 << 17 } else { 0 },
                        ));
                    }
                }
                if !zindex_added {
                    cmds.remove::<ZIndex>();
                }
            }

            // gather scroll events
            if let Some(scroll_entity) = link.scroll_entity {
                if ui_transform.scroll_position != link.scroll_position {
                    // deferred update
                    let pos = ui_transform.scroll_position.clone();
                    commands
                        .entity(bevy_entity)
                        .modify_component(move |link: &mut UiLink| {
                            link.scroll_position = pos;
                        });

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

            // add any blocked elts
            for unblocked_elt in blocked_elements
                .remove(&scene_id)
                .unwrap_or_default()
                .into_iter()
                .rev()
            {
                unprocessed_uis.push_front(unblocked_elt);
            }
        }

        debug!(
            "made ui; placed: {}, unplaced: {} ({:?})",
            valid_nodes.len(),
            blocked_elements.len(),
            blocked_elements
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

#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/show_ui")]
struct ToggleSceneUiCommand {
    hash: String,
    enable: Option<bool>,
}

#[derive(Resource)]
pub struct HiddenSceneUis {
    pub scenes: HashMap<String, bool>,
    pub show_all: bool,
}

impl Default for HiddenSceneUis {
    fn default() -> Self {
        Self {
            scenes: Default::default(),
            show_all: true,
        }
    }
}

fn toggle_scene_ui_command(
    mut input: ConsoleCommand<ToggleSceneUiCommand>,
    live_scenes: Res<LiveScenes>,
    ui_links: Query<(Entity, &UiLink), (With<SceneUiData>, Without<SuperUserScene>)>,
    mut styles: Query<&mut Style>,
    mut hidden_uis: ResMut<HiddenSceneUis>,
) {
    if let Some(Ok(ToggleSceneUiCommand { hash, enable })) = input.take() {
        let all = hash == "all";

        // get final state
        let enable = enable
            .unwrap_or_else(|| !(hidden_uis.scenes.get(&hash).unwrap_or(&hidden_uis.show_all)));

        // if hash is all, toggle everything
        if all {
            hidden_uis.show_all = enable;
            hidden_uis.scenes.clear();
        }

        // get target entity if required
        let target_entity = if hash != "all" {
            let Some(entity) = live_scenes.scenes.get(&hash) else {
                input.reply_failed(format!("{hash} not found in live scenes"));
                return;
            };

            Some(entity)
        } else {
            None
        };

        for (scene_ent, link) in ui_links.iter() {
            // skip non-matching entities
            if target_entity.is_some_and(|e| e != &scene_ent) {
                continue;
            }

            // get the ui root
            let Ok(mut style) = styles.get_mut(link.ui_entity) else {
                if !all {
                    input.reply_failed("failed to obtain ui root entity");
                    return;
                } else {
                    continue;
                }
            };

            // set the target state
            if enable {
                style.display = Display::Flex;
            } else {
                style.display = Display::None;
            };

            // store the scene specific state
            if !all {
                hidden_uis.scenes.insert(hash.clone(), enable);
                input.reply_ok(format!("{hash}: {enable}"));
            }
        }

        input.reply_ok(format!("{hash}: {}", enable));
    }
}
