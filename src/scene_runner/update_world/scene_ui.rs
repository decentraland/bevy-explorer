use std::collections::BTreeSet;

use bevy::{
    prelude::*,
    utils::{HashMap, HashSet},
};

use crate::{
    common::PrimaryUser,
    dcl::interface::ComponentPosition,
    dcl_component::{
        proto_components::sdk::components::{
            PbUiTransform, YgAlign, YgDisplay, YgFlexDirection, YgJustify, YgOverflow,
            YgPositionType, YgUnit, YgWrap,
        },
        SceneComponentId, SceneEntityId,
    },
    scene_runner::{
        renderer_context::RendererSceneContext, ContainingScene, SceneEntity, SceneSets,
    },
    util::TryInsertEx,
};

use super::AddCrdtInterfaceExt;

pub struct SceneUiPlugin;

#[derive(Component, Debug, Clone)]
pub struct UiTransform {
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
    display: Display,
    basis: Val,
    grow: f32,
    size: Size,
    min_size: Size,
    max_size: Size,
    position: UiRect,
    margin: UiRect,
    padding: UiRect,
}

impl From<PbUiTransform> for UiTransform {
    fn from(value: PbUiTransform) -> Self {
        macro_rules! val {
            ($pb:ident, $u:ident, $v:ident) => {
                match $pb.$u() {
                    YgUnit::YguUndefined | YgUnit::YguAuto => Val::Auto,
                    YgUnit::YguPoint => Val::Px($pb.$v),
                    YgUnit::YguPercent => Val::Percent($pb.$v),
                }
            };
        }

        macro_rules! size {
            ($pb:ident, $wu:ident, $w:ident, $hu:ident, $h:ident) => {
                Size {
                    width: val!($pb, $wu, $w),
                    height: val!($pb, $hu, $h),
                }
            };
        }

        macro_rules! rect {
            ($pb:ident, $lu:ident, $l:ident, $ru:ident, $r:ident, $tu:ident, $t:ident, $bu:ident, $b:ident) => {
                UiRect {
                    left: val!($pb, $lu, $l),
                    right: val!($pb, $ru, $r),
                    top: val!($pb, $tu, $t),
                    bottom: val!($pb, $bu, $b),
                }
            };
        }

        Self {
            parent: SceneEntityId::from_proto_u32(value.parent as u32),
            right_of: SceneEntityId::from_proto_u32(value.right_of as u32),
            align_content: match value.align_content() {
                YgAlign::YgaAuto |
                YgAlign::YgaBaseline | // baseline is invalid for align content
                YgAlign::YgaFlexStart => AlignContent::FlexStart,
                YgAlign::YgaCenter => AlignContent::Center,
                YgAlign::YgaFlexEnd => AlignContent::FlexEnd,
                YgAlign::YgaStretch => AlignContent::Stretch,
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
                YgJustify::YgjFlexStart => JustifyContent::Start,
                YgJustify::YgjCenter => JustifyContent::Center,
                YgJustify::YgjFlexEnd => JustifyContent::FlexEnd,
                YgJustify::YgjSpaceBetween => JustifyContent::SpaceBetween,
                YgJustify::YgjSpaceAround => JustifyContent::SpaceAround,
                YgJustify::YgjSpaceEvenly => JustifyContent::SpaceEvenly,
            },
            overflow: match value.overflow() {
                YgOverflow::YgoVisible => Overflow::Visible,
                YgOverflow::YgoHidden => Overflow::Hidden,
                YgOverflow::YgoScroll => {
                    // TODO: map to scroll area
                    warn!("ui overflow scroll not implemented");
                    Overflow::Hidden
                }
            },
            display: match value.display() {
                YgDisplay::YgdFlex => Display::Flex,
                YgDisplay::YgdNone => Display::None,
            },
            basis: val!(value, flex_basis_unit, flex_basis),
            grow: value.flex_grow,
            size: size!(value, width_unit, width, height_unit, height),
            min_size: size!(
                value,
                min_width_unit,
                min_width,
                min_height_unit,
                min_height
            ),
            max_size: size!(
                value,
                max_width_unit,
                max_width,
                max_height_unit,
                max_height
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
                position_bottom
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
                margin_bottom
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
                padding_bottom
            ),
        }
    }
}

impl Plugin for SceneUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbUiTransform, UiTransform>(
            SceneComponentId::UI_TRANSFORM,
            ComponentPosition::EntityOnly,
        );

        app.add_system(init_scene_ui_root.in_set(SceneSets::PostInit));
        app.add_systems(
            (update_scene_ui_components, layout_scene_ui)
                .chain()
                .in_set(SceneSets::PostLoop),
        );
    }
}

#[derive(Component, Default)]
pub struct SceneUiData {
    nodes: BTreeSet<Entity>,
    relayout: bool,
    current_node: Option<Entity>,
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

fn update_scene_ui_components(
    new_entities: Query<(Entity, &SceneEntity), Changed<UiTransform>>,
    mut ui_roots: Query<&mut SceneUiData>,
) {
    for (ent, scene_id) in new_entities.iter() {
        let Ok(mut ui_data) = ui_roots.get_mut(scene_id.root) else {
            warn!("scene root missing for {:?}", scene_id.root);
            continue;
        };

        ui_data.nodes.insert(ent);
        ui_data.relayout = true;
    }
}

fn layout_scene_ui(
    mut commands: Commands,
    mut scene_uis: Query<(Entity, &mut SceneUiData)>,
    player: Query<Entity, With<PrimaryUser>>,
    containing_scene: ContainingScene,
    ui_nodes: Query<(&SceneEntity, &UiTransform)>,
) {
    let current_scene = player
        .get_single()
        .ok()
        .and_then(|p| containing_scene.get(p));

    for (ent, mut ui_data) in scene_uis.iter_mut() {
        if Some(ent) == current_scene {
            if ui_data.relayout || ui_data.current_node.is_none() {
                // remove any old instance of the ui
                if let Some(node) = ui_data.current_node.take() {
                    commands.entity(node).despawn_recursive();
                }

                // collect ui data
                let mut deleted_nodes = HashSet::default();
                let mut unprocessed_uis =
                    HashMap::from_iter(ui_data.nodes.iter().flat_map(|node| {
                        match ui_nodes.get(*node) {
                            Ok((scene_entity, transform)) => {
                                Some((scene_entity.id, transform.clone()))
                            }
                            Err(_) => {
                                // remove this node
                                deleted_nodes.insert(*node);
                                None
                            }
                        }
                    }));

                // remove any dead nodes
                ui_data.nodes.retain(|node| !deleted_nodes.contains(node));

                let mut processed_nodes = HashMap::new();

                let root = commands
                    .spawn(NodeBundle {
                        style: Style {
                            position_type: PositionType::Absolute,
                            position: UiRect::all(Val::Px(0.0)),
                            ..Default::default()
                        },
                        // background_color: Color::BLUE.into(),
                        ..Default::default()
                    })
                    .id();
                processed_nodes.insert(SceneEntityId::ROOT, root);

                let mut modified = true;
                while modified && !unprocessed_uis.is_empty() {
                    modified = false;
                    unprocessed_uis.retain(|scene_id, ui_transform| {
                        // if our rightof is not added, we can't process this node
                        if !processed_nodes.contains_key(&ui_transform.right_of) {
                            println!("can't place {} with ro {}", scene_id, ui_transform.right_of);
                            return true;
                        }

                        // if our parent is not added, we can't process this node
                        let Some(parent) = processed_nodes.get(&ui_transform.parent) else {
                            println!("can't place {} with parent {}", scene_id, ui_transform.parent);
                            return true;
                        };

                        // we can process this node
                        commands.entity(*parent).with_children(|commands| {
                            let new_entity = commands
                                .spawn(NodeBundle {
                                    style: Style {
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
                                        size: ui_transform.size,
                                        min_size: ui_transform.min_size,
                                        max_size: ui_transform.max_size,
                                        position: ui_transform.position,
                                        margin: ui_transform.margin,
                                        padding: ui_transform.padding,
                                        ..Default::default()
                                    },
                                    background_color: Color::WHITE.into(),
                                    ..Default::default()
                                })
                                .id();

                            processed_nodes.insert(*scene_id, new_entity);
                        });

                        // mark to continue and remove from unprocessed
                        modified = true;
                        false
                    });
                }

                println!(
                    "made ui; placed: {}, unplaced: {}",
                    processed_nodes.len(),
                    unprocessed_uis.len()
                );
                ui_data.relayout = false;
                ui_data.current_node = Some(root);
            }
        } else {
            // destroy other uis
            if let Some(current_ui) = ui_data.current_node.take() {
                commands.entity(current_ui).despawn_recursive();
            }
        }
    }
}
