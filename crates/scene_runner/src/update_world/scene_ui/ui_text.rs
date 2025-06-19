use bevy::prelude::*;
use dcl_component::proto_components::{
    sdk::components::{self, PbUiText},
    Color4DclToBevy,
};

use crate::{update_world::text_shape::make_text_section, SceneEntity};

use super::{UiLink, UiTransform};

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
    pub font: components::common::Font,
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
            color: value
                .color
                .map(Color4DclToBevy::convert_srgba)
                .unwrap_or(Color::WHITE),
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
            wrapping: value.text_wrap() == components::TextWrap::TwWrap,
        }
    }
}

#[derive(Component)]
pub struct UiTextMarker;

pub fn set_ui_text(
    mut commands: Commands,
    texts: Query<
        (&SceneEntity, &UiText, &UiTransform, &UiLink),
        Or<(Changed<UiText>, Changed<UiLink>)>,
    >,
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
            for child in children.iter().filter(|c| prev_texts.get(*c).is_ok()) {
                if let Ok(mut commands) = commands.get_entity(child) {
                    commands.despawn();
                }
            }
        }
    }

    for (scene_ent, ui_text, ui_transform, link) in texts.iter() {
        debug!("{} added text {:?}", scene_ent.id, ui_text);

        // remove old text
        if let Ok(children) = children.get(link.ui_entity) {
            for child in children.iter().filter(|c| prev_texts.get(*c).is_ok()) {
                if let Ok(mut commands) = commands.get_entity(child) {
                    commands.despawn();
                }
            }
        }

        if ui_text.text.is_empty() || ui_text.font_size <= 0.0 {
            continue;
        }

        let Ok(mut ent_cmds) = commands.get_entity(link.ui_entity) else {
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
            Node {
                position_type: PositionType::Relative,
                margin: UiRect::all(Val::Px(ui_text.font_size * 0.5)),
                ..Default::default()
            }
        } else {
            Node {
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

        let text_element = ent_cmds
            .commands()
            .spawn((
                Node {
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
                UiTextMarker,
                children!((inner_style, children!((text, ZIndex(1))))),
            ))
            // .try_with_children(|c| {
            //     c.spawn(inner_style)
            //     .try_with_children(|c| {
            //         c.spawn((
            //             text,
            //             ZIndex(1),
            //         ));
            //     });
            // })
            .id();

        ent_cmds.insert_children(0, &[text_element]);
    }
}
