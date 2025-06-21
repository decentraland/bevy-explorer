use bevy::prelude::*;
use common::util::TryPushChildrenEx;
use dcl_component::proto_components::{
    common::{BorderRect, TextureUnion},
    sdk::components::{self, PbUiBackground},
    Color4DclToBevy,
};
use ui_core::{
    nine_slice::Ui9Slice, stretch_uvs_image::StretchUvMaterial, ui_builder::SpawnSpacer,
};

use crate::{
    renderer_context::RendererSceneContext,
    update_world::material::{TextureResolveError, TextureResolver},
    SceneEntity,
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
            color: value.color.map(Color4DclToBevy::convert_srgba),
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

#[derive(Component)]
pub struct RetryBackground;

#[derive(Component)]
pub struct UiMaterialSource(Entity);

pub fn set_ui_background(
    mut commands: Commands,
    backgrounds: Query<
        (Entity, &SceneEntity, &UiBackground, &UiLink),
        Or<(
            Changed<UiBackground>,
            Changed<UiLink>,
            With<RetryBackground>,
        )>,
    >,
    mut removed: RemovedComponents<UiBackground>,
    links: Query<&UiLink>,
    children: Query<&Children>,
    prev_backgrounds: Query<Entity, With<UiBackgroundMarker>>,
    contexts: Query<&RendererSceneContext>,
    mut resolver: TextureResolver,
    mut stretch_uvs: ResMut<Assets<StretchUvMaterial>>,
    sourced: Query<(
        Entity,
        Option<&MaterialNode<StretchUvMaterial>>,
        &UiMaterialSource,
    )>,
) {
    for ent in removed.read() {
        let Ok(link) = links.get(ent) else {
            continue;
        };

        if let Ok(children) = children.get(link.ui_entity) {
            for child in children.iter().filter(|c| prev_backgrounds.get(*c).is_ok()) {
                if let Ok(mut commands) = commands.get_entity(child) {
                    commands.despawn();
                }
            }
        }

        if let Ok(mut commands) = commands.get_entity(link.ui_entity) {
            commands.insert(BackgroundColor::DEFAULT);
        }
    }

    for (ent, scene_ent, background, link) in backgrounds.iter() {
        if let Ok(children) = children.get(link.ui_entity) {
            for child in children.iter().filter(|c| prev_backgrounds.get(*c).is_ok()) {
                if let Ok(mut commands) = commands.get_entity(child) {
                    commands.despawn();
                }
            }
        }

        commands.entity(ent).remove::<RetryBackground>();

        let Ok(mut commands) = commands.get_entity(link.ui_entity) else {
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
                .map(|tex| resolver.resolve_texture(ctx, tex));
            if let Some(Err(TextureResolveError::SourceNotReady)) = image.as_ref() {
                commands.commands().entity(ent).insert(RetryBackground);
                continue;
            }
            let image = image.and_then(|r| r.ok());

            if let Some(image) = image {
                let image_color = background.color.unwrap_or(Color::WHITE);
                let image_color = image_color.with_alpha(image_color.alpha() * link.opacity.0);

                let background_entity = match texture.mode {
                    BackgroundTextureMode::NineSlices(rect) => commands
                        .commands()
                        .spawn((
                            Node {
                                position_type: PositionType::Absolute,
                                top: Val::Px(0.0),
                                right: Val::Px(0.0),
                                left: Val::Px(0.0),
                                bottom: Val::Px(0.0),
                                overflow: Overflow::clip(),
                                ..Default::default()
                            },
                            Ui9Slice {
                                image: image.image,
                                center_region: rect.into(),
                                tint: Some(image_color),
                            },
                            UiBackgroundMarker,
                        ))
                        .id(),
                    BackgroundTextureMode::Stretch(ref uvs) => commands
                        .commands()
                        .spawn((
                            Node {
                                position_type: PositionType::Absolute,
                                top: Val::Px(0.0),
                                right: Val::Px(0.0),
                                left: Val::Px(0.0),
                                bottom: Val::Px(0.0),
                                overflow: Overflow::clip(),
                                ..Default::default()
                            },
                            UiBackgroundMarker,
                        ))
                        .try_with_children(|c| {
                            let mut inner = c.spawn((
                                Node {
                                    position_type: PositionType::Absolute,
                                    top: Val::Px(0.0),
                                    right: Val::Px(0.0),
                                    left: Val::Px(0.0),
                                    bottom: Val::Px(0.0),
                                    ..Default::default()
                                },
                                MaterialNode(stretch_uvs.add(StretchUvMaterial {
                                    image: image.image.clone(),
                                    uvs: *uvs,
                                    color: image_color.to_linear().to_vec4(),
                                })),
                            ));
                            if let Some(source) = image.source_entity {
                                inner.insert(UiMaterialSource(source));
                            }
                        })
                        .id(),
                    BackgroundTextureMode::Center => commands
                        .commands()
                        .spawn((
                            Node {
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
                            UiBackgroundMarker,
                        ))
                        .try_with_children(|c| {
                            c.spacer();
                            c.spawn(Node {
                                flex_direction: FlexDirection::Column,
                                justify_content: JustifyContent::Center,
                                overflow: Overflow::clip(),
                                height: Val::Percent(100.0),
                                ..Default::default()
                            })
                            .try_with_children(|c| {
                                c.spacer();
                                let mut inner = c.spawn((
                                    Node {
                                        overflow: Overflow::clip(),
                                        ..Default::default()
                                    },
                                    ImageNode::new(image.image).with_color(image_color),
                                ));
                                if let Some(source) = image.source_entity {
                                    inner.insert(UiMaterialSource(source));
                                }
                                c.spacer();
                            });
                            c.spacer();
                        })
                        .id(),
                };

                commands.insert_children(0, &[background_entity]);
            } else {
                warn!("failed to load ui image from content map: {:?}", texture);
            }
        } else if let Some(color) = background.color {
            commands.insert(BackgroundColor(color));
        }
    }

    for (ent, maybe_stretch, source) in sourced.iter() {
        if commands.get_entity(source.0).is_err() {
            commands.entity(ent).insert(RetryBackground);
        } else if let Some(h_stretch) = maybe_stretch {
            stretch_uvs.get_mut(h_stretch);
        }
    }
}
