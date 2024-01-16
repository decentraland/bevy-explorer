/* PbTextShape

    text: string;
foundation: works
bevy: works

    font?: Font | undefined;
does nothing

    fontSize?: number | undefined;
foundation: works.
bevy: works
- note that width W is specified in meters
- assuming P pixels per meter on the rendered text texture, a given fontSize of F should correspond to a rendered font size (in points) of "F * P / 10"
- e.g. if you use 100 px per meter, a font size of 10 should use underlying font size 100
- e.g. if you use 200 px per meter, a font size of 10 should use underlying font size 200

    fontAutoSize?: boolean | undefined;
foundation and bevy:
- doesn't change font size
- disables textWrapping

    textAlign?: TextAlignMode | undefined;
kinda of works
foundation + bevy: text align left / center / right justify the text
foundation + bevy: - text align top/middle/bottom change the anchor of the text block (so the quad y is (0, 1), (-0.5, 0.5), (-1, 0) respectively

    width?: number | undefined;
foundation + bevy: when used with textWrapping, specifies the wrap size in meters. otherwise no effect

    height?: number | undefined;
does nothing

    paddingTop?: number | undefined;
foundation: changes the y offset by -paddingTop, units unclear
bevy: changes the y offset by -paddingTop in meters

    paddingRight?: number | undefined;
foundation: reduces width
bevy: adds padding

    paddingBottom?: number | undefined;
foundation: changes the y offset by +paddingBottom, units unclear
bevy: changes the y offset by +paddingBottom in meters

    paddingLeft?: number | undefined;
foundation + bevy: adds padding to the left!

    lineSpacing?: number | undefined;
foundation: 100 adds a full line of space, less than 10 does nothing.
bevy: not implemented

    lineCount?: number | undefined;
foundation: when textWrapping is true, works. causes some strange re-interpretation of paddingTop/paddingBottom.
bevy: acts like height

    textWrapping?: boolean | undefined;
foundation + bevy: works

    shadowBlur?: number | undefined;
foundation: flaky when changed. when shadowOffsetX=1, a range around 1-5 is useable. suggest use some standard shadow when != 0
bevy: not implemented

    shadowOffsetX?: number | undefined;
foundation: when 0, disables shadows. otherwise affects shadowBlur in a non-linear way, bigger X -> smaller blur. doesn't change shadow offset at all.
bevy: not implemented

    shadowOffsetY?: number | undefined;
does nothing

    outlineWidth?: number | undefined;
foundation: changes outline thickness. units unclear. 0.15 seems to make something around half the letter size. larger than 0.25 just obscures the whole text. probably just check for non-zero and apply some standard outline in that case.
bevy: not implemented

    shadowColor?: Color3 | undefined;
foundation: works
bevy: not implemented

    outlineColor?: Color3 | undefined;
foundation: works
bevy: not implemented

    textColor?: Color4 | undefined;
foundation: works
bevy: not implemented


*/

use bevy::prelude::*;
use common::sets::SceneLoopSets;
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::sdk::components::{common::TextAlignMode, PbTextShape},
    SceneComponentId,
};
use ui_core::{
    ui_builder::SpawnSpacer, TEXT_SHAPE_FONT_MONO, TEXT_SHAPE_FONT_SANS, TEXT_SHAPE_FONT_SERIF,
};
use world_ui::WorldUi;

use crate::{renderer_context::RendererSceneContext, SceneEntity};

use super::AddCrdtInterfaceExt;

pub struct TextShapePlugin;

impl Plugin for TextShapePlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbTextShape, TextShape>(
            SceneComponentId::TEXT_SHAPE,
            ComponentPosition::EntityOnly,
        );
        app.add_systems(
            Update,
            update_text_shapes.in_set(SceneLoopSets::UpdateWorld),
        );
    }
}

#[derive(Component, Clone, PartialEq)]
pub struct TextShape(pub PbTextShape);

impl From<PbTextShape> for TextShape {
    fn from(value: PbTextShape) -> Self {
        Self(value)
    }
}

const PIX_PER_M: f32 = 100.0;

fn update_text_shapes(
    mut commands: Commands,
    query: Query<(Entity, &SceneEntity, &TextShape), Changed<TextShape>>,
    mut removed: RemovedComponents<TextShape>,
    scenes: Query<&RendererSceneContext>,
) {
    // remove deleted ui nodes
    for e in removed.read() {
        if let Some(mut commands) = commands.get_entity(e) {
            commands.remove::<WorldUi>();
        }
    }

    // add new nodes
    for (ent, scene_ent, text_shape) in query.iter() {
        let bounds = scenes
            .get(scene_ent.root)
            .map(|c| c.bounds)
            .unwrap_or_default();

        debug!("ts: {:?}", text_shape.0);

        let text_align = text_shape
            .0
            .text_align
            .map(|_| text_shape.0.text_align())
            .unwrap_or(TextAlignMode::TamMiddleCenter);

        let valign = match text_align {
            TextAlignMode::TamTopLeft
            | TextAlignMode::TamTopCenter
            | TextAlignMode::TamTopRight => -0.5,
            TextAlignMode::TamMiddleLeft
            | TextAlignMode::TamMiddleCenter
            | TextAlignMode::TamMiddleRight => 0.0,
            TextAlignMode::TamBottomLeft
            | TextAlignMode::TamBottomCenter
            | TextAlignMode::TamBottomRight => 0.5,
        };

        let halign = match text_align {
            TextAlignMode::TamTopLeft
            | TextAlignMode::TamMiddleLeft
            | TextAlignMode::TamBottomLeft => TextAlignment::Left,
            TextAlignMode::TamTopCenter
            | TextAlignMode::TamMiddleCenter
            | TextAlignMode::TamBottomCenter => TextAlignment::Center,
            TextAlignMode::TamTopRight
            | TextAlignMode::TamMiddleRight
            | TextAlignMode::TamBottomRight => TextAlignment::Right,
        };

        let add_y_pix = (text_shape.0.padding_bottom() - text_shape.0.padding_top()) * PIX_PER_M;

        let font_size = text_shape.0.font_size.unwrap_or(10.0) * PIX_PER_M * 0.1;

        let wrapping = text_shape.0.text_wrapping() && !text_shape.0.font_auto_size();

        let width = if wrapping {
            (text_shape.0.width.unwrap_or(1.0) * PIX_PER_M) as u32
        } else {
            4096
        };
        let resize_width =
            (text_shape.0.width.is_none() || !wrapping).then_some(ResizeAxis::MaxContent);

        let max_height = match text_shape.0.line_count() {
            0 => 4096,
            lines => lines as u32 * font_size as u32,
        };

        // create ui layout
        let mut text = Text::from_section(
            text_shape.0.text.as_str(),
            TextStyle {
                font_size,
                color: text_shape
                    .0
                    .text_color
                    .map(Into::into)
                    .unwrap_or(Color::WHITE),
                font: match text_shape.0.font() {
                    dcl_component::proto_components::sdk::components::common::Font::FSansSerif => {
                        &TEXT_SHAPE_FONT_SANS
                    }
                    dcl_component::proto_components::sdk::components::common::Font::FSerif => {
                        &TEXT_SHAPE_FONT_SERIF
                    }
                    dcl_component::proto_components::sdk::components::common::Font::FMonospace => {
                        &TEXT_SHAPE_FONT_MONO
                    }
                }
                .get()
                .unwrap()
                .clone(),
            },
        )
        .with_alignment(halign);

        if !wrapping {
            text = text.with_no_wrap();
        }

        let ui_root = commands
            .spawn((NodeBundle {
                style: Style {
                    flex_direction: FlexDirection::Row,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..Default::default()
                },
                // background_color: Color::rgba(1.0, 0.0, 0.0, 0.25).into(),
                ..Default::default()
            },))
            .with_children(|c| {
                if text_shape.0.padding_left.is_some() {
                    c.spawn(NodeBundle {
                        style: Style {
                            width: Val::Px(text_shape.0.padding_left() * PIX_PER_M),
                            min_width: Val::Px(text_shape.0.padding_left() * PIX_PER_M),
                            max_width: Val::Px(text_shape.0.padding_left() * PIX_PER_M),
                            ..Default::default()
                        },
                        ..Default::default()
                    });
                }

                if halign != TextAlignment::Left {
                    c.spacer();
                }

                c.spawn(NodeBundle {
                    // background_color: Color::rgba(0.0, 0.0, 1.0, 0.25).into(),
                    ..Default::default()
                })
                .with_children(|c| {
                    c.spawn(TextBundle {
                        text,
                        style: Style {
                            align_self: match halign {
                                TextAlignment::Left => AlignSelf::FlexStart,
                                TextAlignment::Center => AlignSelf::Center,
                                TextAlignment::Right => AlignSelf::FlexEnd,
                            },
                            ..Default::default()
                        },
                        ..Default::default()
                    });
                });

                if halign != TextAlignment::Right {
                    c.spacer();
                }

                if text_shape.0.padding_right.is_some() {
                    c.spawn(NodeBundle {
                        style: Style {
                            width: Val::Px(text_shape.0.padding_right() * PIX_PER_M),
                            min_width: Val::Px(text_shape.0.padding_right() * PIX_PER_M),
                            max_width: Val::Px(text_shape.0.padding_right() * PIX_PER_M),
                            ..Default::default()
                        },
                        ..Default::default()
                    });
                }
            })
            .id();

        commands.entity(ent).try_insert(WorldUi {
            width,
            height: max_height,
            resize_width,
            resize_height: Some(ResizeAxis::MaxContent),
            pix_per_m: PIX_PER_M,
            valign,
            add_y_pix,
            bounds,
            ui_root,
            dispose_ui: true,
        });
    }
}
