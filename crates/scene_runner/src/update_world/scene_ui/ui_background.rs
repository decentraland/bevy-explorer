use bevy::prelude::*;
use common::util::TryPushChildrenEx;
use dcl_component::proto_components::{
    common::{texture_union, BorderRect, TextureUnion},
    sdk::components::{self, PbUiBackground},
};
use ui_core::{
    nine_slice::Ui9Slice, stretch_uvs_image::StretchUvMaterial, ui_builder::SpawnSpacer,
};

use crate::{
    renderer_context::RendererSceneContext, update_world::material::TextureResolver, SceneEntity,
};

use super::UiLink;

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

#[derive(Component)]
pub struct UiBackgroundMarker;

pub fn set_ui_background(
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
                                UiBackgroundMarker,
                            ));
                        });
                    }
                    BackgroundTextureMode::Stretch(ref uvs) => {
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
                                UiBackgroundMarker,
                            ))
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
                            c.spawn((
                                NodeBundle {
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
                                },
                                UiBackgroundMarker,
                            ))
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
