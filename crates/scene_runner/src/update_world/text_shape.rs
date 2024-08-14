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

use bevy::{core::FrameCount, prelude::*, text::BreakLineOn, utils::hashbrown::HashMap};
use common::{
    sets::SceneLoopSets,
    util::{DespawnWith, TryPushChildrenEx},
};
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::sdk::components::{common::TextAlignMode, PbTextShape},
    SceneComponentId,
};
use ui_core::{ui_builder::SpawnSpacer, user_font, FontName, WeightName};
use world_ui::{spawn_world_ui_view, WorldUi};

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

const PIX_PER_M: f32 = 200.0;

#[derive(Component)]
pub struct PriorTextShapeUi(Entity);

#[derive(Component, Clone, Copy)]
pub struct SceneWorldUi {
    view: Entity,
    ui_root: Entity,
}

fn update_text_shapes(
    mut commands: Commands,
    images: ResMut<Assets<Image>>,
    query: Query<(Entity, &SceneEntity, &TextShape, Option<&PriorTextShapeUi>), Changed<TextShape>>,
    mut removed: RemovedComponents<TextShape>,
    scenes: Query<(&RendererSceneContext, Option<&SceneWorldUi>)>,
    frame: Res<FrameCount>,
) {
    // remove deleted ui nodes
    for e in removed.read() {
        if let Some(mut commands) = commands.get_entity(e) {
            commands.remove::<WorldUi>();
        }
        debug!("[{}] kill textshape {e:?}", frame.0);
    }

    let mut new_world_uis: HashMap<Entity, SceneWorldUi> = HashMap::default();
    let images = images.into_inner();

    // add new nodes
    for (ent, scene_ent, text_shape, maybe_prior) in query.iter() {
        debug!("ts: {:?}", text_shape.0);

        let Ok((scene, world_ui)) = scenes.get(scene_ent.root) else {
            warn!("no scene!");
            continue;
        };

        let world_ui = world_ui.unwrap_or_else(|| {
            new_world_uis.entry(scene_ent.root).or_insert_with(|| {
                let view = spawn_world_ui_view(&mut commands, images);
                commands.entity(view).insert(DespawnWith(ent));
                let ui_root = commands
                    .spawn((
                        NodeBundle {
                            style: Style {
                                width: Val::Px(8192.0),
                                min_width: Val::Px(8192.0),
                                max_width: Val::Px(8192.0),
                                max_height: Val::Px(8192.0),
                                flex_direction: FlexDirection::Row,
                                flex_wrap: FlexWrap::Wrap,
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                        TargetCamera(view),
                        DespawnWith(ent),
                    ))
                    .id();
                let world_ui = SceneWorldUi { view, ui_root };
                commands.entity(scene_ent.root).try_insert(world_ui);
                world_ui
            })
        });

        if let Some(prior) = maybe_prior {
            if let Some(commands) = commands.get_entity(prior.0) {
                commands.despawn_recursive();
            }
        }

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

        let (halign_wui, halign) = match text_align {
            TextAlignMode::TamTopLeft
            | TextAlignMode::TamMiddleLeft
            | TextAlignMode::TamBottomLeft => (0.5, JustifyText::Left),
            TextAlignMode::TamTopCenter
            | TextAlignMode::TamMiddleCenter
            | TextAlignMode::TamBottomCenter => (0.0, JustifyText::Center),
            TextAlignMode::TamTopRight
            | TextAlignMode::TamMiddleRight
            | TextAlignMode::TamBottomRight => (-0.5, JustifyText::Right),
        };

        let add_y_pix = (text_shape.0.padding_bottom() - text_shape.0.padding_top()) * PIX_PER_M;

        let font_size = text_shape.0.font_size.unwrap_or(10.0) * PIX_PER_M * 0.1;

        let wrapping = text_shape.0.text_wrapping() && !text_shape.0.font_auto_size();

        let width = if wrapping {
            text_shape.0.width.unwrap_or(1.0) * PIX_PER_M
        } else {
            4096.0
        };

        // create ui layout
        let text = make_text_section(
            text_shape.0.text.as_str(),
            font_size,
            text_shape
                .0
                .text_color
                .map(Into::into)
                .unwrap_or(Color::WHITE),
            text_shape.0.font(),
            halign,
            wrapping,
        );

        let ui_node = commands
            .spawn((
                NodeBundle {
                    style: Style {
                        flex_direction: FlexDirection::Row,
                        max_width: Val::Px(width),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                DespawnWith(ent),
            ))
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

                if halign != JustifyText::Left {
                    c.spacer();
                }

                c.spawn(NodeBundle {
                    ..Default::default()
                })
                .with_children(|c| {
                    c.spawn(TextBundle {
                        text,
                        style: Style {
                            align_self: match halign {
                                JustifyText::Left => AlignSelf::FlexStart,
                                JustifyText::Center => AlignSelf::Center,
                                JustifyText::Right => AlignSelf::FlexEnd,
                            },
                            ..Default::default()
                        },
                        ..Default::default()
                    });
                });

                if halign != JustifyText::Right {
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

        commands
            .entity(world_ui.ui_root)
            .try_push_children(&[ui_node]);

        commands.entity(ent).try_insert((
            PriorTextShapeUi(ui_node),
            WorldUi {
                dbg: format!("TextShape `{}`", text_shape.0.text),
                pix_per_m: PIX_PER_M,
                valign,
                halign: halign_wui,
                add_y_pix,
                bounds: scene.bounds,
                view: world_ui.view,
                ui_node,
            },
        ));

        debug!("[{}] textshape {ent:?}", frame.0);
    }
}

pub fn make_text_section(
    text: &str,
    font_size: f32,
    color: Color,
    font: dcl_component::proto_components::sdk::components::common::Font,
    justify: JustifyText,
    wrapping: bool,
) -> Text {
    let text = text.replace("\\n", "\n");

    let font_name = match font {
        dcl_component::proto_components::sdk::components::common::Font::FSansSerif => {
            FontName::Serif
        }
        dcl_component::proto_components::sdk::components::common::Font::FSerif => FontName::Sans,
        dcl_component::proto_components::sdk::components::common::Font::FMonospace => {
            FontName::Mono
        }
    };

    // split by <b>s and <i>s
    let mut b_count = 0usize;
    let mut i_count = 0usize;
    let mut b_offset = text.find("<b>");
    let mut i_offset = text.find("<i>");
    let mut xb_offset = text.find("</b>");
    let mut xi_offset = text.find("</i>");
    let mut section_start = 0;

    let mut sections = Vec::default();

    loop {
        let section_end = [b_offset, i_offset, xb_offset, xi_offset]
            .iter()
            .fold(usize::MAX, |c, o| c.min(o.unwrap_or(c)));
        let weight = match (b_count, i_count) {
            (0, 0) => WeightName::Regular,
            (0, _) => WeightName::Italic,
            (_, 0) => WeightName::Bold,
            (_, _) => WeightName::BoldItalic,
        };

        if section_end == usize::MAX {
            sections.push(TextSection::new(
                &text[section_start..],
                TextStyle {
                    font: user_font(font_name, weight),
                    font_size,
                    color,
                },
            ));
            break;
        }

        sections.push(TextSection::new(
            &text[section_start..section_end],
            TextStyle {
                font: user_font(font_name, weight),
                font_size,
                color,
            },
        ));

        match &text[section_end..section_end + 3] {
            "<b>" => {
                b_count += 1;
                b_offset = text[section_end + 1..]
                    .find("<b>")
                    .map(|v| v + section_end + 1);
                section_start = section_end + 3;
            }
            "<i>" => {
                i_count += 1;
                i_offset = text[section_end + 1..]
                    .find("<i>")
                    .map(|v| v + section_end + 1);
                section_start = section_end + 3;
            }
            "</b" => {
                b_count = b_count.saturating_sub(1);
                xb_offset = text[section_end + 1..]
                    .find("</b>")
                    .map(|v| v + section_end + 1);
                section_start = section_end + 4;
            }
            "</i" => {
                i_count = i_count.saturating_sub(1);
                xi_offset = text[section_end + 1..]
                    .find("</i>")
                    .map(|v| v + section_end + 1);
                section_start = section_end + 4;
            }
            _ => {
                error!("{}", &text[section_end..=section_end + 2]);
                panic!()
            }
        }
    }

    Text {
        sections,
        linebreak_behavior: if wrapping {
            BreakLineOn::WordBoundary
        } else {
            BreakLineOn::NoWrap
        },
        justify,
    }
}
