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
    pbr::{ExtendedMaterial, MaterialExtension, NotShadowCaster},
    prelude::*,
    render::render_resource::{
        AsBindGroup, Extent3d, ShaderRef, ShaderType, TextureDimension, TextureFormat,
        TextureUsages,
    },
    utils::HashMap,
};
use common::{sets::SceneSets, util::TryPushChildrenEx};
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::sdk::components::{common::TextAlignMode, PbTextShape},
    SceneComponentId,
};
use ui_core::TEXT_SHAPE_FONT;

use crate::{renderer_context::RendererSceneContext, SceneEntity};

use super::{
    scene_material::{SceneBound, SceneMaterial},
    AddCrdtInterfaceExt,
};

pub struct TextShapePlugin;

impl Plugin for TextShapePlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbTextShape, TextShape>(
            SceneComponentId::TEXT_SHAPE,
            ComponentPosition::EntityOnly,
        );
        app.add_systems(Update, update_text_shapes.in_set(SceneSets::PostLoop));
        app.add_plugins(MaterialPlugin::<TextShapeMaterial>::default());
    }
}

#[derive(Component)]
pub struct TextShape(pub PbTextShape);

impl From<PbTextShape> for TextShape {
    fn from(value: PbTextShape) -> Self {
        Self(value)
    }
}

const PIX_PER_M: f32 = 100.0;

#[allow(clippy::too_many_arguments)]
fn update_text_shapes(
    mut commands: Commands,
    query: Query<(Entity, &SceneEntity, &TextShape), Changed<TextShape>>,
    mut removed: RemovedComponents<TextShape>,
    scenes: Query<&RendererSceneContext>,
    mut images: ResMut<Assets<Image>>,
    mut old_items: Local<HashMap<Entity, Vec<Entity>>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TextShapeMaterial>>,
    mut camera_query: Query<&mut ResizeTarget>,
    quad_query: Query<&Handle<TextShapeMaterial>>,
) {
    // remove deleted nodes
    for e in removed.read().flat_map(|r| old_items.remove(&r)).flatten() {
        if let Some(commands) = commands.get_entity(e) {
            commands.despawn_recursive();
        }
    }

    // add new nodes
    for (ent, scene_ent, text_shape) in query.iter() {
        let bounds = scenes
            .get(scene_ent.root)
            .map(|c| c.bounds)
            .unwrap_or_default();

        println!("ts: {:?}", text_shape.0);

        let text_align = text_shape
            .0
            .text_align
            .map(|_| text_shape.0.text_align())
            .unwrap_or(TextAlignMode::TamMiddleCenter);

        let valign = match text_align {
            TextAlignMode::TamTopLeft
            | TextAlignMode::TamTopCenter
            | TextAlignMode::TamTopRight => 0.5,
            TextAlignMode::TamMiddleLeft
            | TextAlignMode::TamMiddleCenter
            | TextAlignMode::TamMiddleRight => 0.0,
            TextAlignMode::TamBottomLeft
            | TextAlignMode::TamBottomCenter
            | TextAlignMode::TamBottomRight => -0.5,
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

        let resize_width =
            (text_shape.0.width.is_none() || !wrapping).then_some(ResizeAxis::MaxContent);
        
        let max_height = match text_shape.0.line_count {
            Some(lines) => lines as u32 * font_size as u32,
            None => 4096,
        };
        
        let image_size = Extent3d {
            width: (text_shape.0.width.unwrap_or(1.0) * PIX_PER_M) as u32,
            height: max_height,
            depth_or_array_layers: 1,
        };

        // create or update camera and quad
        let (camera, quad) = if let Some(prev_items) = old_items.get(&ent) {
            let mut prev_items = prev_items.iter();
            let (camera, ui, quad) = (
                prev_items.next().unwrap(),
                prev_items.next().unwrap(),
                prev_items.next().unwrap(),
            );

            // update camera
            if let Ok(mut target) = camera_query.get_mut(*camera) {
                target.width = resize_width;
            }

            if let Ok(quad) = quad_query.get(*quad) {
                if let Some(mat) = materials.get_mut(quad) {
                    // update valign
                    mat.extension.data.valign = valign;

                    // update image
                    if let Some(image) =
                        images.get_mut(mat.base.base.base_color_texture.clone().unwrap())
                    {
                        image.resize(image_size)
                    }
                }
            }

            // remove previous ui node
            if let Some(commands) = commands.get_entity(*ui) {
                commands.despawn_recursive();
            }
            (*camera, *quad)
        } else {
            // create render target image (it'll be resized)
            let mut image = Image::new_fill(
                image_size,
                TextureDimension::D2,
                &[0, 0, 0, 0],
                TextureFormat::Bgra8UnormSrgb,
            );
            image.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT;
            let image = images.add(image);

            let camera = commands
                .spawn((
                    Camera2dBundle {
                        camera: Camera {
                            order: -1,
                            target: bevy::render::camera::RenderTarget::Image(image.clone()),
                            ..Default::default()
                        },
                        camera_2d: Camera2d {
                            clear_color: bevy::core_pipeline::clear_color::ClearColorConfig::Custom(
                                Color::NONE,
                            ),
                        },
                        ..Default::default()
                    },
                    ResizeTarget {
                        width: resize_width,
                        height: Some(ResizeAxis::MaxContent),
                        info: ResizeInfo {
                            min_width: None,
                            max_width: Some(4096),
                            min_height: None,
                            max_height: Some(max_height),
                            viewport_reference_size: UVec2::new(1024, 1024),
                        },
                    },
                ))
                .id();

            let quad = commands
                .spawn((
                    MaterialMeshBundle {
                        mesh: meshes.add(shape::Quad::default().into()),
                        material: materials.add(TextShapeMaterial {
                            base: SceneMaterial {
                                base: StandardMaterial {
                                    base_color_texture: Some(image),
                                    unlit: true,
                                    alpha_mode: AlphaMode::Blend,
                                    ..Default::default()
                                },
                                extension: SceneBound { bounds },
                            },
                            extension: TextQuad {
                                data: TextQuadData {
                                    valign,
                                    pix_per_m: PIX_PER_M,
                                    add_y_pix,
                                },
                            },
                        }),
                        ..Default::default()
                    },
                    NotShadowCaster,
                ))
                .id();

            commands.entity(ent).try_push_children(&[quad]);

            (camera, quad)
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
                font: TEXT_SHAPE_FONT.get().unwrap().clone(),
            },
        )
        .with_alignment(halign);

        if !wrapping {
            text = text.with_no_wrap();
        }

        let ui = commands
            .spawn((
                NodeBundle {
                    style: Style {
                        flex_direction: FlexDirection::Row,
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        ..Default::default()
                    },
                    // background_color: Color::rgba(1.0, 0.0, 0.0, 0.25).into(),
                    ..Default::default()
                },
                TargetCamera(camera),
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

                c.spawn((NodeBundle::default(), TargetCamera(camera)))
                    .with_children(|c| {
                        c.spawn(TextBundle {
                            text,
                            ..Default::default()
                        });
                    });

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

        old_items.insert(ent, vec![camera, ui, quad]);
    }
}

pub type TextShapeMaterial = ExtendedMaterial<SceneMaterial, TextQuad>;

#[derive(Asset, TypePath, Clone, AsBindGroup)]
pub struct TextQuad {
    #[uniform(200)]
    pub data: TextQuadData,
}

#[derive(Clone, ShaderType)]
pub struct TextQuadData {
    pub valign: f32,
    pub pix_per_m: f32,
    pub add_y_pix: f32,
}

impl MaterialExtension for TextQuad {
    fn vertex_shader() -> bevy::render::render_resource::ShaderRef {
        ShaderRef::Path("shaders/text_quad_vertex.wgsl".into())
    }

    fn prepass_vertex_shader() -> ShaderRef {
        ShaderRef::Path("shaders/text_quad_vertex.wgsl".into())
    }
}
