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

use bevy::{
    diagnostic::FrameCount,
    ecs::{relationship::RelatedSpawner, spawn::SpawnWith},
    platform::collections::{HashMap, HashSet},
    prelude::*,
    text::{ComputedTextBlock, CosmicBuffer, LineBreak},
    ui::{update::update_clipping_system, widget::text_system},
};
use common::{
    sets::SceneLoopSets,
    util::{DespawnWith, TryPushChildrenEx},
};
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::{
        sdk::components::{common::TextAlignMode, PbTextShape},
        Color4DclToBevy,
    },
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
        app.add_systems(
            PostUpdate,
            apply_text_extras
                .after(text_system)
                .after(TransformSystem::TransformPropagate)
                .before(update_clipping_system),
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
pub struct PriorTextShapeUi(Entity, PbTextShape);

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
        if let Ok(mut commands) = commands.get_entity(e) {
            commands.remove::<WorldUi>();
        }
        debug!("[{}] kill textshape {e:?}", frame.0);
    }

    let mut new_world_uis: HashMap<Entity, SceneWorldUi> = HashMap::new();
    let images = images.into_inner();

    // add new nodes
    for (ent, scene_ent, text_shape, maybe_prior) in query.iter() {
        debug!("ts: {:?}", text_shape.0);

        let Ok((scene, world_ui)) = scenes.get(scene_ent.root) else {
            warn!("no scene!");
            continue;
        };

        if let Some(prior) = maybe_prior {
            if prior.1 == text_shape.0 {
                continue;
            }

            if let Ok(mut commands) = commands.get_entity(prior.0) {
                commands.despawn();
            }
        }

        if text_shape.0.text.is_empty() || text_shape.0.font_size.is_some_and(|size| size <= 0.0) {
            continue;
        }

        let world_ui = world_ui.unwrap_or_else(|| {
            new_world_uis.entry(scene_ent.root).or_insert_with(|| {
                let (view, _) = spawn_world_ui_view(&mut commands, images, None);
                commands.entity(view).insert(DespawnWith(ent));
                let ui_root = commands
                    .spawn((
                        Node {
                            width: Val::Px(8192.0),
                            min_width: Val::Px(8192.0),
                            max_width: Val::Px(8192.0),
                            max_height: Val::Px(8192.0),
                            flex_direction: FlexDirection::Row,
                            flex_wrap: FlexWrap::Wrap,
                            align_items: AlignItems::FlexStart,
                            align_content: AlignContent::FlexStart,
                            ..Default::default()
                        },
                        UiTargetCamera(view),
                        DespawnWith(ent),
                    ))
                    .id();
                let world_ui = SceneWorldUi { view, ui_root };
                commands.entity(scene_ent.root).try_insert(world_ui);
                world_ui
            })
        });

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

        let font_size = 30.0;

        let wrapping = text_shape.0.text_wrapping() && !text_shape.0.font_auto_size();

        let width = if wrapping {
            text_shape.0.width.unwrap_or(1.0) * PIX_PER_M
        } else {
            4096.0
        };

        // create ui layout
        let source = if text_shape.0.text.len() > 2048 {
            warn!(
                "textshape text truncated from {} to 2048 chars",
                text_shape.0.text.len()
            );
            &text_shape.0.text.as_str()[0..2048]
        } else {
            text_shape.0.text.as_str()
        };
        let text = make_text_section(
            source,
            font_size,
            text_shape
                .0
                .text_color
                .map(Color4DclToBevy::convert_srgba)
                .unwrap_or(Color::WHITE),
            text_shape.0.font(),
            halign,
            wrapping,
        );

        let ui_node = commands
            .spawn((
                Node {
                    margin: UiRect::all(Val::Px(1.0)),
                    flex_direction: FlexDirection::Row,
                    max_width: Val::Px(width),
                    ..Default::default()
                },
                DespawnWith(ent),
            ))
            .with_children(|c| {
                if text_shape.0.padding_left.is_some() {
                    c.spawn(Node {
                        width: Val::Px(text_shape.0.padding_left() * PIX_PER_M),
                        min_width: Val::Px(text_shape.0.padding_left() * PIX_PER_M),
                        max_width: Val::Px(text_shape.0.padding_left() * PIX_PER_M),
                        ..Default::default()
                    });
                }

                if halign != JustifyText::Left {
                    c.spacer();
                }

                c.spawn(Node::default()).with_child((
                    text,
                    Node {
                        align_self: match halign {
                            JustifyText::Left => AlignSelf::FlexStart,
                            JustifyText::Center => AlignSelf::Center,
                            JustifyText::Right => AlignSelf::FlexEnd,
                            JustifyText::Justified => AlignSelf::Center,
                        },
                        ..Default::default()
                    },
                ));

                if halign != JustifyText::Right {
                    c.spacer();
                }

                if text_shape.0.padding_right.is_some() {
                    c.spawn(Node {
                        width: Val::Px(text_shape.0.padding_right() * PIX_PER_M),
                        min_width: Val::Px(text_shape.0.padding_right() * PIX_PER_M),
                        max_width: Val::Px(text_shape.0.padding_right() * PIX_PER_M),
                        ..Default::default()
                    });
                }
            })
            .id();

        if let Ok(mut commands) = commands.get_entity(world_ui.ui_root) {
            commands.try_push_children(&[ui_node]);
        }

        commands.entity(ent).try_insert((
            PriorTextShapeUi(ui_node, text_shape.0.clone()),
            WorldUi {
                dbg: format!("TextShape `{source}`"),
                pix_per_m: 375.0 / text_shape.0.font_size.unwrap_or(10.0),
                valign,
                halign: halign_wui,
                add_y_pix,
                bounds: scene.bounds.clone(),
                view: world_ui.view,
                ui_node,
                vertex_billboard: false,
                blend_mode: AlphaMode::Blend,
            },
        ));

        debug!("[{}] textshape {ent:?}", frame.0);
    }
}

#[derive(Component)]
pub struct TextExtraMarker;

#[inline]
fn round_ties_up(value: f32) -> f32 {
    if value.fract() != -0.5 {
        value.round()
    } else {
        value.ceil()
    }
}

#[inline]
fn round_layout_coords(value: Vec2) -> Vec2 {
    Vec2 {
        x: round_ties_up(value.x),
        y: round_ties_up(value.y),
    }
}
fn apply_text_extras(
    mut commands: Commands,
    text: Query<(
        Entity,
        &Children,
        &GlobalTransform,
        Ref<TextLayout>,
        Ref<ComputedTextBlock>,
        Ref<ComputedNode>,
        Option<Ref<UiTargetCamera>>,
    )>,
    changed_spans: Query<
        &ChildOf,
        (
            With<TextExtras>,
            Or<(Changed<TextSpan>, Changed<TextExtras>)>,
        ),
    >,
    spans: Query<(&TextSpan, &TextColor, Option<&TextExtras>)>,
    existing: Query<(), With<TextExtraMarker>>,
    mut removed: RemovedComponents<TextExtras>,
) {
    let find_bounds = |buffer: &CosmicBuffer, text: &str, section: (usize, usize)| -> Vec<Vec4> {
        let mut segments = Vec::default();
        let preceding_text = &text[..section.0];
        let start_line = preceding_text.chars().filter(|c| *c == '\n').count();
        let start_line_index = preceding_text
            .char_indices()
            .rfind(|(_, c)| *c == '\n')
            .map(|(ix, _)| ix)
            .unwrap_or(0);
        let start_section_index = preceding_text
            .char_indices()
            .last()
            .map(|(ix, _)| ix)
            .unwrap_or(0)
            - start_line_index;

        let section_text = &text[section.0..section.1];
        let end_line = start_line + section_text.chars().filter(|c| *c == '\n').count();
        let end_line_index = section_text
            .char_indices()
            .last()
            .map(|(ix, _)| ix)
            .unwrap_or(0);
        let end_section_index = section_text
            .char_indices()
            .rfind(|(_, c)| *c == '\n')
            .map(|(ix, _)| end_line_index - ix)
            .unwrap_or(start_section_index + end_line_index + 1);

        let mut segment_y = f32::NEG_INFINITY;
        let runs = buffer
            .layout_runs()
            .skip_while(|run| run.line_i < start_line)
            .take_while(|run| run.line_i <= end_line);

        for run in runs {
            let glyphs = run
                .glyphs
                .iter()
                .skip_while(|g| run.line_i == start_line && g.start < start_section_index)
                .take_while(|g| run.line_i < end_line || g.end <= end_section_index);

            for glyph in glyphs {
                debug!("g: {},{}", glyph.x, glyph.y);
                if run.line_top + glyph.y != segment_y {
                    segments.push(Vec4::new(
                        glyph.x,
                        run.line_top + glyph.y,
                        glyph.w,
                        run.line_height,
                    ));
                    segment_y = run.line_top + glyph.y;
                } else {
                    let segment = segments.last_mut().unwrap();
                    segment.z = glyph.x + glyph.w - segment.x;
                }
            }
        }

        segments
    };

    let removed_extras = removed.read().collect::<HashSet<_>>();
    let changed = changed_spans
        .iter()
        .map(|c| c.parent())
        .collect::<HashSet<_>>();

    for (entity, children, gt, layout, computed_text, computed_node, maybe_camera) in text.iter() {
        let mut requires_update =
            layout.is_changed() || computed_text.is_changed() || computed_node.is_changed();
        requires_update |= changed.contains(&entity);
        requires_update |= children.iter().any(|child| removed_extras.contains(&child));

        if !requires_update {
            continue;
        }

        // remove prior
        for child in children {
            if existing.get(*child).is_ok() {
                commands.entity(*child).despawn();
            }
        }

        let mut make_mark = |bound: Vec4, color: Color, top: f32, height: f32| -> Entity {
            // because we make marks based on calculated text positions, we have to run after the ui layout functions
            // but that means our marks won't be positioned until next frame. if text is deleted/replaced every frame
            // then it never shows.
            // so we do the layouting ourselves, copying bevy's `update_uinode_geometry_recursive`, and set the visibility explicitly.
            // we have to also add the equivalent style settings, or it will be overwritten next frame.
            let mut view_visibility = ViewVisibility::default();
            view_visibility.set();
            let height = (bound.w * height).max(1.0);
            let size = Vec2::new(bound.z, height);
            let parent_tl = gt.translation().truncate() - computed_node.size * 0.5;
            let my_tl = parent_tl + Vec2::new(bound.x, bound.y + bound.w * top);
            let my_translation = round_layout_coords(my_tl + size * 0.5);
            let mut cmds = commands.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(bound.x),
                    top: Val::Px(bound.y + bound.w * top),
                    width: Val::Px(bound.z),
                    height: Val::Px(height),
                    ..Default::default()
                },
                BackgroundColor(color),
                ZIndex(1),
                view_visibility,
                ComputedNode {
                    stack_index: computed_node.stack_index + 1,
                    size: round_layout_coords(size),
                    outline_width: 0.0,
                    outline_offset: 0.0,
                    unrounded_size: size,
                    content_size: size,
                    border: BorderRect::ZERO,
                    border_radius: ResolvedBorderRadius::ZERO,
                    padding: BorderRect::ZERO,
                    inverse_scale_factor: computed_node.inverse_scale_factor,
                },
                GlobalTransform::from_translation(my_translation.extend(0.0)),
                TextExtraMarker,
            ));

            if let Some(target_camera) = maybe_camera.as_ref() {
                cmds.insert(UiTargetCamera(target_camera.0));
            }

            cmds.id()
        };

        let mut text = String::default();
        let mut spanned_extras = Vec::default();
        for &child in children {
            let Ok((span, color, maybe_extras)) = spans.get(child) else {
                continue;
            };

            if let Some(extras) = maybe_extras {
                spanned_extras.push((extras, color.0, text.len(), text.len() + span.0.len()));
            }
            text.push_str(span.0.as_str());
        }

        let buffer = computed_text.buffer();
        let mut ents = Vec::default();

        for (extras, text_color, start, end) in spanned_extras {
            let bounds = find_bounds(buffer, &text, (start, end));
            for bound in bounds {
                if extras.strike {
                    ents.push(make_mark(bound, text_color, 0.6, 0.1));
                }
                if extras.underline {
                    ents.push(make_mark(bound, text_color, 0.95, 0.1));
                }
                if let Some(color) = extras.mark {
                    ents.push(make_mark(bound, color, 0.0, 1.0));
                }
            }
        }

        commands.entity(entity).try_push_children(ents.as_slice());
    }
}

#[derive(Component, Default, Clone, Copy)]
pub struct TextExtras {
    strike: bool,
    underline: bool,
    mark: Option<Color>,
}

pub fn make_text_section(
    text: &str,
    font_size: f32,
    color: Color,
    font: dcl_component::proto_components::sdk::components::common::Font,
    justify: JustifyText,
    wrapping: bool,
) -> impl Bundle {
    let text = text.replace("\\n", "\n");

    let font_name = match font {
        dcl_component::proto_components::sdk::components::common::Font::FSansSerif => {
            FontName::Sans
        }
        dcl_component::proto_components::sdk::components::common::Font::FSerif => FontName::Serif,
        dcl_component::proto_components::sdk::components::common::Font::FMonospace => {
            FontName::Mono
        }
    };

    // split by <b>s and <i>s
    let mut b_count = 0usize;
    let mut i_count = 0usize;
    let mut u_count = 0usize;
    let mut s_count = 0usize;
    let mut marks = Vec::<Color>::default();
    let mut override_colors = Vec::<Color>::default();
    let mut section_start = 0usize;

    let mut spans = Vec::default();

    while section_start < text.len() {
        // read initial tags
        while text[section_start..].starts_with('<') {
            if let Some((close, _)) = text[section_start..]
                .char_indices()
                .find(|(_, c)| *c == '>')
            {
                let tag = text[section_start + 1..section_start + close]
                    .trim()
                    .to_ascii_lowercase();
                match tag.as_str() {
                    "b" => b_count += 1,
                    "i" => i_count += 1,
                    "s" => s_count += 1,
                    "u" => u_count += 1,
                    "/b" => b_count = b_count.saturating_sub(1),
                    "/i" => i_count = i_count.saturating_sub(1),
                    "/s" => s_count = s_count.saturating_sub(1),
                    "/u" => u_count = u_count.saturating_sub(1),
                    i if i.get(0..4) == Some("mark") => {
                        marks.push(
                            i.get(5..)
                                .and_then(|color| Srgba::hex(color).map(Color::from).ok())
                                .unwrap_or_else(|| {
                                    warn!("unrecognised mark color `{i}`");
                                    let mut mark_color = color;
                                    mark_color.set_alpha(color.alpha() * 0.5);
                                    mark_color
                                }),
                        );
                    }
                    "/mark" => {
                        marks.pop();
                    }
                    i if i.get(0..5) == Some("color") => {
                        override_colors.push(
                            i.get(6..)
                                .and_then(|color| Srgba::hex(color).map(Color::from).ok())
                                .unwrap_or_else(|| {
                                    warn!("unrecognised text color `{i}`");
                                    color
                                }),
                        );
                    }
                    "/color" => {
                        override_colors.pop();
                    }
                    _ => warn!("unrecognised text tag `{tag}`"),
                }
                section_start = section_start + close + 1;
            } else {
                break;
            }
        }

        let weight = match (b_count, i_count) {
            (0, 0) => WeightName::Regular,
            (0, _) => WeightName::Italic,
            (_, 0) => WeightName::Bold,
            (_, _) => WeightName::BoldItalic,
        };

        let mut maybe_extras: Option<TextExtras> = None;

        if s_count > 0 {
            maybe_extras.get_or_insert_default().strike = true;
        }
        if u_count > 0 {
            maybe_extras.get_or_insert_default().underline = true;
        }
        if let Some(mark) = marks.last().as_ref() {
            maybe_extras.get_or_insert_default().mark = Some(**mark);
        }

        let section_end = text[section_start..]
            .char_indices()
            .find(|(_, c)| *c == '<')
            .map(|(ix, _)| section_start + ix.max(1))
            .unwrap_or(usize::MAX);

        let font = TextFont {
            font: user_font(font_name, weight),
            font_size: font_size * 0.95,
            ..Default::default()
        };

        let color = TextColor(override_colors.last().copied().unwrap_or(color));

        let span = if section_end == usize::MAX {
            TextSpan::new(&text[section_start..])
        } else {
            TextSpan::new(&text[section_start..section_end])
        };

        spans.push((span, font, color, maybe_extras));

        section_start = section_end;
    }

    let f = move |parent: &mut RelatedSpawner<ChildOf>| {
        for (span, font, color, maybe_extras) in spans {
            let mut child = parent.spawn((span, font, color));
            if let Some(extras) = maybe_extras {
                child.insert(extras);
            }
        }
    };

    (
        Text::default(),
        TextLayout::new(
            justify,
            if wrapping {
                LineBreak::WordBoundary
            } else {
                LineBreak::NoWrap
            },
        ),
        Children::spawn(SpawnWith(f)),
    )
}
