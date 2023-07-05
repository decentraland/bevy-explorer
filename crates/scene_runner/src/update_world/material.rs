use bevy::{pbr::NotShadowCaster, prelude::*};

use crate::{renderer_context::RendererSceneContext, ContainerEntity, SceneSets};
use common::util::TryInsertEx;
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::{
        common::{texture_union, TextureUnion, VideoTexture},
        sdk::components::{pb_material, MaterialTransparencyMode, PbMaterial},
    },
    SceneComponentId, SceneEntityId,
};
use ipfs::IpfsLoaderExt;

use super::AddCrdtInterfaceExt;

pub struct MaterialDefinitionPlugin;

#[derive(Component, Debug, Default, Clone)]
pub struct MaterialDefinition {
    pub material: StandardMaterial,
    pub shadow_caster: bool,
    pub base_color_texture: Option<TextureUnion>,
}

impl From<PbMaterial> for MaterialDefinition {
    fn from(value: PbMaterial) -> Self {
        let (material, base_color_texture) = match &value.material {
            Some(pb_material::Material::Unlit(unlit)) => {
                let base_color = unlit.diffuse_color.map(Color::from).unwrap_or(Color::WHITE);

                let alpha_mode = if base_color.a() < 1.0 {
                    AlphaMode::Blend
                } else {
                    AlphaMode::Opaque
                };

                (
                    StandardMaterial {
                        base_color,
                        double_sided: true,
                        cull_mode: None,
                        unlit: true,
                        alpha_mode,
                        ..Default::default()
                    },
                    unlit.texture.clone(),
                )
            }
            Some(pb_material::Material::Pbr(pbr)) => {
                let base_color = pbr.albedo_color.map(Color::from).unwrap_or(Color::WHITE);

                let alpha_mode = match pbr
                    .transparency_mode
                    .map(MaterialTransparencyMode::from_i32)
                    .unwrap_or(None)
                {
                    Some(MaterialTransparencyMode::MtmOpaque) => AlphaMode::Opaque,
                    Some(MaterialTransparencyMode::MtmAlphaTest) => {
                        AlphaMode::Mask(pbr.alpha_test.unwrap_or(0.5))
                    }
                    Some(MaterialTransparencyMode::MtmAlphaBlend) => AlphaMode::Blend,
                    Some(MaterialTransparencyMode::MtmAlphaTestAndAlphaBlend) => unimplemented!(), // TODO requires bevy patch or custom material or material extension tbd
                    Some(MaterialTransparencyMode::MtmAuto) | None => {
                        if base_color.a() < 1.0 {
                            AlphaMode::Blend
                        } else {
                            AlphaMode::Opaque
                        }
                    }
                };

                (
                    StandardMaterial {
                        base_color,
                        emissive: pbr.emissive_color.map(Color::from).unwrap_or(Color::BLACK),
                        // TODO what is pbr.reflectivity_color?
                        metallic: pbr.metallic.unwrap_or(0.5),
                        perceptual_roughness: pbr.roughness.unwrap_or(0.5),
                        // TODO glossiness
                        // TODO intensities
                        double_sided: true,
                        cull_mode: None,
                        alpha_mode,
                        ..Default::default()
                    },
                    pbr.texture.clone(),
                )
            }
            None => Default::default(),
        };

        let shadow_caster = match value.material {
            Some(pb_material::Material::Unlit(unlit)) => unlit.cast_shadows,
            Some(pb_material::Material::Pbr(pbr)) => pbr.cast_shadows,
            _ => None,
        }
        .unwrap_or(true);

        Self {
            material,
            shadow_caster,
            base_color_texture,
        }
    }
}

impl Plugin for MaterialDefinitionPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbMaterial, MaterialDefinition>(
            SceneComponentId::MATERIAL,
            ComponentPosition::EntityOnly,
        );

        app.add_system(update_materials.in_set(SceneSets::PostLoop));
    }
}

#[derive(Component)]
pub struct RetryMaterial;

#[derive(Component)]
pub struct TouchMaterial;

#[derive(Component)]
pub struct VideoTextureOutput(pub Handle<Image>);

fn update_materials(
    mut commands: Commands,
    mut new_materials: Query<
        (Entity, &MaterialDefinition, &ContainerEntity),
        Or<(Changed<MaterialDefinition>, With<RetryMaterial>)>,
    >,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
    scenes: Query<&RendererSceneContext>,
    videos: Query<&VideoTextureOutput>,
    touch: Query<&Handle<StandardMaterial>, With<TouchMaterial>>,
) {
    for (ent, defn, container) in new_materials.iter_mut() {
        // get texture
        let base_color_texture = if let Some(TextureUnion {
            tex: Some(texture_union::Tex::Texture(texture)),
        }) = defn.base_color_texture.as_ref()
        {
            scenes.get(container.root).ok().and_then(|root| {
                // TODO handle wrapmode and filtering once we have some asset processing pipeline in place (bevy 0.11-0.12)
                asset_server
                    .load_content_file::<Image>(&texture.src, &root.hash)
                    .ok()
            })
        } else if let Some(TextureUnion {
            tex: Some(texture_union::Tex::VideoTexture(VideoTexture{ video_player_entity, .. })),
        }) = defn.base_color_texture.as_ref()
        {
            let Some(video_entity) = scenes.get(container.root).ok().and_then(|root| {
                root.bevy_entity(SceneEntityId::from_proto_u32(*video_player_entity))
            }) else {
                warn!("failed to look up video source entity");
                continue;
            };
            
            if let Ok(vt) = videos.get(video_entity) {
                println!("adding video texture {:?}", vt.0);
                commands.entity(ent).insert(TouchMaterial);
                Some(vt.0.clone())
            } else {
                warn!("video source entity not ready, retrying ...");
                commands.entity(ent).insert(RetryMaterial);
                continue;
            }
        } else {
            None
        };

        // info!("found a mat for {ent:?}");
        let mut commands = commands.entity(ent);
        commands.remove::<RetryMaterial>().try_insert(materials.add(StandardMaterial {
            base_color_texture,
            ..defn.material.clone()
        }));
        if defn.shadow_caster {
            commands.remove::<NotShadowCaster>();
        } else {
            commands.try_insert(NotShadowCaster);
        }
    }

    for touch in touch.iter() {
        materials.get_mut(touch);
    }
}
