use std::sync::OnceLock;

use bevy::{
    ecs::system::SystemParam,
    image::{
        ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler,
        ImageSamplerDescriptor,
    },
    math::Affine2,
    pbr::NotShadowCaster,
    prelude::*,
    render::primitives::Aabb,
};
use common::{structs::AppConfig, util::AsH160};
use comms::profile::ProfileManager;
use ipfs::{ipfs_path::IpfsPath, IpfsAssetServer};

use crate::{
    gltf_resolver::GltfMaterialResolver, renderer_context::RendererSceneContext,
    update_scene::pointer_results::ResolveCursor, ContainerEntity, SceneEntity, SceneSets,
};
use dcl::interface::{ComponentPosition, CrdtType};
use dcl_component::{
    proto_components::{
        common::{texture_union, TextureFilterMode, TextureUnion, TextureWrapMode, Vector2},
        sdk::components::{pb_material, MaterialTransparencyMode, PbMaterial},
        Color3BevyToDcl, Color3DclToBevy, Color4BevyToDcl, Color4DclToBevy,
    },
    SceneComponentId, SceneEntityId,
};
use scene_material::{SceneBound, SceneMaterial};

use super::{mesh_renderer::update_mesh, scene_ui::UiTextureOutput, AddCrdtInterfaceExt};

pub struct MaterialDefinitionPlugin;

#[derive(Component, Clone)]
pub struct BaseMaterial {
    pub material: StandardMaterial,
    pub gltf: String,
    pub name: String,
}

#[derive(Debug, Default, Clone)]
pub struct MaterialDefinition {
    pub material: StandardMaterial,
    pub shadow_caster: bool,
    pub base_color_texture: Option<TextureUnion>,
    pub emmissive_texture: Option<TextureUnion>,
    pub normal_map: Option<TextureUnion>,
}

#[derive(Component, Clone)]
pub struct PbMaterialComponent(pub PbMaterial);

impl From<PbMaterial> for PbMaterialComponent {
    fn from(value: PbMaterial) -> Self {
        Self(value)
    }
}

static DEFAULT_BASE: OnceLock<StandardMaterial> = OnceLock::new();

impl MaterialDefinition {
    pub fn from_base_and_material(base: Option<&BaseMaterial>, pb_material: &PbMaterial) -> Self {
        let base = base
            .map(|b| &b.material)
            .unwrap_or(DEFAULT_BASE.get_or_init(|| StandardMaterial {
                base_color: Color::WHITE,
                emissive: LinearRgba::BLACK,
                perceptual_roughness: 0.5,
                metallic: 0.5,
                reflectance: 0.5,
                ..Default::default()
            }));

        let (material, base_color_texture, emmissive_texture, normal_map) = match &pb_material
            .material
        {
            Some(pb_material::Material::Unlit(unlit)) => {
                let base_color = unlit
                    .diffuse_color
                    .map(Color4DclToBevy::convert_linear_rgba)
                    .unwrap_or(base.base_color);

                let alpha_mode = if base_color.alpha() < 1.0 {
                    AlphaMode::Blend
                } else if let Some(test) = unlit.alpha_test {
                    AlphaMode::Mask(test)
                } else {
                    AlphaMode::Opaque
                };

                let inner_texture = unlit
                    .texture
                    .as_ref()
                    .and_then(|t| t.tex.as_ref())
                    .and_then(|t| match t {
                        texture_union::Tex::Texture(texture) => Some(texture),
                        _ => None,
                    });
                let uv_transform = Affine2 {
                    matrix2: Mat2::from_diagonal(
                        inner_texture
                            .and_then(|t| t.tiling)
                            .map(|t| Vec2::from(&t))
                            .unwrap_or(Vec2::ONE),
                    ),
                    translation: inner_texture
                        .and_then(|t| t.offset)
                        .map(|o| Vec2::from(&o) * Vec2::new(1.0, -1.0))
                        .unwrap_or(Vec2::ZERO),
                };

                (
                    StandardMaterial {
                        base_color,
                        unlit: true,
                        alpha_mode,
                        uv_transform,
                        ..base.clone()
                    },
                    unlit.texture.clone(),
                    None,
                    None,
                )
            }
            Some(pb_material::Material::Pbr(pbr)) => {
                if pbr.alpha_texture.is_some()
                    && pbr.texture.is_some()
                    && pbr.alpha_texture != pbr.texture
                {
                    debug!("separate alpha texture not supported");
                }

                let base_color = pbr
                    .albedo_color
                    .map(Color4DclToBevy::convert_linear_rgba)
                    .unwrap_or(base.base_color);

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
                    Some(MaterialTransparencyMode::MtmAlphaTestAndAlphaBlend) => {
                        // TODO requires bevy patch or custom material or material extension tbd
                        warn!(
                            "MaterialTransparencyMode::MtmAlphaTestAndAlphaBlend not implemented!"
                        );
                        AlphaMode::Blend
                    }
                    Some(MaterialTransparencyMode::MtmAuto) | None => {
                        if base_color.alpha() < 1.0 || pbr.alpha_texture.is_some() {
                            AlphaMode::Blend
                        } else if let Some(test) = pbr.alpha_test {
                            AlphaMode::Mask(test)
                        } else {
                            AlphaMode::Opaque
                        }
                    }
                };

                let emissive_intensity = pbr.emissive_intensity.unwrap_or(2.0);
                let emissive = if let Some(color) = pbr.emissive_color {
                    color.convert_linear_rgb().to_linear() * emissive_intensity
                } else if pbr.emissive_texture.is_some() {
                    Color::WHITE.to_linear() * emissive_intensity
                } else {
                    Color::BLACK.to_linear()
                };

                (
                    StandardMaterial {
                        base_color,
                        emissive,
                        // TODO what is pbr.reflectivity_color?
                        metallic: pbr.metallic.unwrap_or(base.metallic),
                        perceptual_roughness: pbr.roughness.unwrap_or(base.perceptual_roughness),
                        // TODO specular intensity
                        alpha_mode,
                        ..base.clone()
                    },
                    pbr.texture.clone(),
                    pbr.emissive_texture.clone(),
                    pbr.bump_texture.clone(),
                )
            }
            None => {
                // use base defaults
                (base.clone(), None, None, None)
            }
        };

        let shadow_caster = match &pb_material.material {
            Some(pb_material::Material::Unlit(unlit)) => unlit.cast_shadows,
            Some(pb_material::Material::Pbr(pbr)) => pbr.cast_shadows,
            _ => None,
        }
        .unwrap_or(true);

        Self {
            material,
            shadow_caster,
            base_color_texture,
            emmissive_texture,
            normal_map,
        }
    }
}

impl Plugin for MaterialDefinitionPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbMaterial, PbMaterialComponent>(
            SceneComponentId::MATERIAL,
            ComponentPosition::EntityOnly,
        );

        app.add_systems(
            Update,
            (update_materials, update_bias)
                .chain()
                .in_set(SceneSets::PostLoop)
                // we must run after update_mesh as that inserts a default material if none is present
                .after(update_mesh),
        );
    }
}

#[derive(Component)]
pub struct RetryMaterial(pub Vec<Handle<Image>>);

#[derive(Component)]
pub struct MaterialSource(pub Entity);

#[derive(Component)]
pub struct VideoTextureOutput(pub Handle<Image>);

#[derive(Debug)]
pub enum TextureResolveError {
    SourceNotAvailable,
    SourceNotReady,
    SceneNotFound,
    AvatarNotFound,
    NotImplemented,
}

#[derive(SystemParam)]
pub struct TextureResolver<'w, 's> {
    ipfas: IpfsAssetServer<'w, 's>,
    videos: Query<'w, 's, &'static VideoTextureOutput>,
    uis: Query<'w, 's, &'static UiTextureOutput>,
    profiles: ProfileManager<'w, 's>,
}

#[derive(Debug)]
pub struct ResolvedTexture {
    pub image: Handle<Image>,
    pub source_entity: Option<Entity>,
    pub camera_target: Option<ResolveCursor>,
}

impl TextureResolver<'_, '_> {
    pub fn resolve_texture(
        &mut self,
        scene: &RendererSceneContext,
        texture: &texture_union::Tex,
    ) -> Result<ResolvedTexture, TextureResolveError> {
        match texture {
            texture_union::Tex::Texture(texture) => {
                let filter_mode = texture
                    .filter_mode
                    .and_then(TextureFilterMode::from_i32)
                    .unwrap_or(TextureFilterMode::TfmBilinear);
                let filter_mode = match filter_mode {
                    TextureFilterMode::TfmPoint => ImageFilterMode::Nearest,
                    TextureFilterMode::TfmBilinear => ImageFilterMode::Linear,
                    TextureFilterMode::TfmTrilinear => ImageFilterMode::Linear,
                };

                let wrap_mode = texture
                    .wrap_mode
                    .and_then(TextureWrapMode::from_i32)
                    .unwrap_or(TextureWrapMode::TwmClamp);
                let wrap_mode = match wrap_mode {
                    TextureWrapMode::TwmRepeat => ImageAddressMode::Repeat,
                    TextureWrapMode::TwmClamp => ImageAddressMode::ClampToEdge,
                    TextureWrapMode::TwmMirror => ImageAddressMode::MirrorRepeat,
                };

                // TODO handle different wrapmode and filtering for the same image at some point...
                Ok(ResolvedTexture {
                    image: self
                        .ipfas
                        .load_content_file_with_settings::<Image, _>(
                            &texture.src,
                            &scene.hash,
                            move |s: &mut ImageLoaderSettings| {
                                s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                                    address_mode_u: wrap_mode,
                                    address_mode_v: wrap_mode,
                                    address_mode_w: wrap_mode,
                                    mag_filter: filter_mode,
                                    min_filter: filter_mode,
                                    mipmap_filter: filter_mode,
                                    ..default()
                                })
                            },
                        )
                        .unwrap(),
                    source_entity: None,
                    camera_target: None,
                })
            }
            texture_union::Tex::AvatarTexture(at) => {
                let h160 = at
                    .user_id
                    .as_h160()
                    .ok_or(TextureResolveError::AvatarNotFound)?;
                let image = self
                    .profiles
                    .get_image(h160)
                    .map_err(|_| TextureResolveError::AvatarNotFound)?
                    .ok_or(TextureResolveError::SourceNotReady)?;

                Ok(ResolvedTexture {
                    image,
                    source_entity: None,
                    camera_target: None,
                })
            }
            texture_union::Tex::VideoTexture(vt) => {
                let Some(video_entity) =
                    scene.bevy_entity(SceneEntityId::from_proto_u32(vt.video_player_entity))
                else {
                    warn!("failed to look up video source entity");
                    return Err(TextureResolveError::SourceNotAvailable);
                };

                if let Ok(vt) = self.videos.get(video_entity) {
                    debug!("adding video texture {:?}", vt.0);
                    Ok(ResolvedTexture {
                        image: vt.0.clone(),
                        source_entity: Some(video_entity),
                        camera_target: None,
                    })
                } else {
                    debug!("video source entity not ready, retrying ...");
                    Err(TextureResolveError::SourceNotReady)
                }
            }
            texture_union::Tex::UiTexture(uit) => {
                let Some(ui_entity) =
                    scene.bevy_entity(SceneEntityId::from_proto_u32(uit.ui_canvas_entity))
                else {
                    warn!("failed to look up ui source entity");
                    return Err(TextureResolveError::SourceNotAvailable);
                };

                match self.uis.get(ui_entity) {
                    Ok(ui_t) => Ok(ResolvedTexture {
                        image: ui_t.image.clone(),
                        source_entity: Some(ui_t.camera),
                        camera_target: Some(ResolveCursor {
                            camera: ui_t.camera,
                            texture_size: ui_t.texture_size.as_vec2(),
                        }),
                    }),
                    Err(_) => {
                        debug!("ui source entity not ready, retrying ...");
                        Err(TextureResolveError::SourceNotReady)
                    }
                }
            }
        }
    }
}

#[allow(clippy::type_complexity)]
fn update_materials(
    mut commands: Commands,
    mut new_materials: Query<
        (
            Entity,
            &PbMaterialComponent,
            &ContainerEntity,
            &SceneEntity,
            Option<&BaseMaterial>,
        ),
        Or<(
            Changed<PbMaterialComponent>,
            Changed<BaseMaterial>,
            With<RetryMaterial>,
        )>,
    >,
    mut materials: ResMut<Assets<SceneMaterial>>,
    sourced: Query<(Entity, &MeshMaterial3d<SceneMaterial>, &MaterialSource)>,
    mut resolver: TextureResolver,
    mut scenes: Query<&mut RendererSceneContext>,
    config: Res<AppConfig>,
    mut gltf_resolver: GltfMaterialResolver,
    images: Res<Assets<Image>>,
) {
    gltf_resolver.begin_frame();

    for (ent, mat, container, scene_ent, base) in new_materials.iter_mut() {
        let new_base;
        let base = if let Some(gltf_def) = mat.0.gltf.as_ref() {
            if base.is_some_and(|b| b.gltf == gltf_def.gltf_src && b.name == gltf_def.name) {
                base
            } else {
                let Ok(scene_hash) = scenes.get(container.root).map(|scene| &scene.hash) else {
                    continue;
                };
                match gltf_resolver.resolve_material(&gltf_def.gltf_src, scene_hash, &gltf_def.name)
                {
                    Err(e) => {
                        warn!("base not found: {e:?}");
                        None
                    }
                    Ok(None) => {
                        // retry
                        commands.entity(ent).insert(RetryMaterial(Vec::default()));
                        continue;
                    }
                    Ok(Some(mat)) => {
                        new_base = BaseMaterial {
                            material: mat.clone(),
                            gltf: gltf_def.gltf_src.clone(),
                            name: gltf_def.name.clone(),
                        };
                        commands.entity(ent).insert(new_base.clone());
                        Some(&new_base)
                    }
                }
            }
        } else {
            None
        };

        let defn = MaterialDefinition::from_base_and_material(base, &mat.0);
        let textures: Result<Vec<_>, _> = [
            &defn.base_color_texture,
            &defn.emmissive_texture,
            &defn.normal_map,
        ]
        .into_iter()
        .map(
            |texture| match texture.as_ref().and_then(|t| t.tex.as_ref()) {
                Some(texture) => {
                    let scene = scenes.get(container.root).map_err(|_| ())?;
                    match resolver.resolve_texture(scene, texture) {
                        Ok(resolved) => Ok(Some(resolved)),
                        Err(TextureResolveError::SourceNotReady) => Err(()),
                        Err(_) => Ok(None),
                    }
                }
                None => Ok(None),
            },
        )
        .collect();

        let textures = match textures {
            Ok(textures) => textures,
            _ => {
                commands.entity(ent).insert(RetryMaterial(Vec::default()));
                continue;
            }
        };

        if let Some(source) = textures
            .iter()
            .flatten()
            .filter_map(|t| t.source_entity)
            .next()
        {
            commands.entity(ent).insert(MaterialSource(source));
        }

        let [mut base_color_texture, emissive_texture, normal_map_texture]: [Option<
            ResolvedTexture,
        >; 3] = textures.try_into().unwrap();

        if let Some(bct) = base_color_texture.as_mut() {
            if let Some(cursor) = bct.camera_target.take() {
                commands.entity(ent).insert(cursor);
            }
        }

        let bounds = scenes
            .get(container.root)
            .map(|c| c.bounds.clone())
            .unwrap_or_default();

        let mut commands = commands.entity(ent);
        commands
            .remove::<RetryMaterial>()
            .try_insert(MeshMaterial3d(
                materials.add(SceneMaterial {
                    base: StandardMaterial {
                        base_color_texture: base_color_texture
                            .map(|t| t.image)
                            .or(base.and_then(|b| b.material.base_color_texture.clone())),
                        emissive_texture: emissive_texture
                            .map(|t| t.image)
                            .or(base.and_then(|b| b.material.emissive_texture.clone())),
                        normal_map_texture: normal_map_texture
                            .map(|t| t.image)
                            .or(base.and_then(|b| b.material.normal_map_texture.clone())),
                        ..defn.material.clone()
                    },
                    extension: SceneBound::new(bounds, config.graphics.oob),
                }),
            ));
        if defn.shadow_caster {
            commands.remove::<NotShadowCaster>();
        } else {
            commands.try_insert(NotShadowCaster);
        }

        // write material back if required
        if mat.0.material.is_none() {
            if let Some(base) = base.as_ref() {
                let Ok(mut scene) = scenes.get_mut(container.root) else {
                    continue;
                };

                scene.update_crdt(
                    SceneComponentId::MATERIAL,
                    CrdtType::LWW_ANY,
                    scene_ent.id,
                    &PbMaterial {
                        material: Some(dcl_material_from_standard_material(
                            &base.material,
                            &images,
                        )),
                        gltf: mat.0.gltf.clone(),
                    },
                );
            }
        }
    }

    for (ent, touch, source) in sourced.iter() {
        if commands.get_entity(source.0).is_err() {
            commands.entity(ent).insert(RetryMaterial(Vec::default()));
        } else {
            materials.get_mut(touch);
        }
    }
}

#[allow(clippy::type_complexity)]
fn update_bias(
    mut materials: ResMut<Assets<SceneMaterial>>,
    query: Query<
        (&Aabb, &MeshMaterial3d<SceneMaterial>),
        Or<(Changed<MeshMaterial3d<SceneMaterial>>, Changed<Aabb>)>,
    >,
) {
    for (aabb, h_material) in query.iter() {
        if let Some(material) = materials.get_mut(h_material) {
            if material.base.alpha_mode == AlphaMode::Blend {
                // add a bias based on the aabb size, to force an explicit transparent order which is
                // hopefully correct, but should be better than nothing even if not always perfect
                material.base.depth_bias = aabb.half_extents.length() * 1e-5;
            }
        }
    }
}

pub fn dcl_material_from_standard_material(
    base: &StandardMaterial,
    images: &Assets<Image>,
) -> pb_material::Material {
    let dcl_texture = |h: &Handle<Image>| -> TextureUnion {
        let path = h.path().unwrap().path();
        let ipfs_path = IpfsPath::new_from_path(path).unwrap().unwrap();
        let src = ipfs_path.content_path().unwrap().to_owned();
        let sampler = if let Some(Image {
            sampler: ImageSampler::Descriptor(d),
            ..
        }) = images.get(h)
        {
            d
        } else {
            &ImageSamplerDescriptor::default()
        };

        let (scale, _, translation) = base.uv_transform.to_scale_angle_translation();
        let tiling = (scale != Vec2::ONE).then_some(Vector2::from(scale));
        let offset = (translation != Vec2::ZERO)
            .then_some(Vector2::from(translation * Vec2::new(1.0, -1.0)));

        TextureUnion {
            tex: Some(dcl_component::proto_components::common::texture_union::Tex::Texture(dcl_component::proto_components::common::Texture {
                src,
                wrap_mode: Some(match sampler.address_mode_u {
                    ImageAddressMode::ClampToEdge => dcl_component::proto_components::common::TextureWrapMode::TwmClamp,
                    ImageAddressMode::Repeat => dcl_component::proto_components::common::TextureWrapMode::TwmRepeat,
                    ImageAddressMode::MirrorRepeat => dcl_component::proto_components::common::TextureWrapMode::TwmMirror,
                    ImageAddressMode::ClampToBorder => dcl_component::proto_components::common::TextureWrapMode::TwmClamp,
                } as i32),
                filter_mode: Some(match sampler.mag_filter {
                    ImageFilterMode::Nearest => dcl_component::proto_components::common::TextureFilterMode::TfmPoint,
                    ImageFilterMode::Linear => dcl_component::proto_components::common::TextureFilterMode::TfmBilinear,
                } as i32),
                offset,
                tiling,
            })),
        }
    };

    let alpha_test = if let AlphaMode::Mask(m) = base.alpha_mode {
        Some(m)
    } else {
        None
    };

    if base.unlit {
        pb_material::Material::Unlit(pb_material::UnlitMaterial {
            texture: base.base_color_texture.as_ref().map(dcl_texture),
            alpha_test,
            cast_shadows: Some(true),
            diffuse_color: Some(base.base_color.convert_linear_rgba()),
        })
    } else {
        pb_material::Material::Pbr(pb_material::PbrMaterial {
            texture: base.base_color_texture.as_ref().map(dcl_texture),
            alpha_test,
            cast_shadows: Some(true),
            alpha_texture: base.base_color_texture.as_ref().map(dcl_texture),
            emissive_texture: base.emissive_texture.as_ref().map(dcl_texture),
            bump_texture: base.normal_map_texture.as_ref().map(dcl_texture),
            albedo_color: Some(base.base_color.convert_linear_rgba()),
            emissive_color: Some(Color::LinearRgba(base.emissive * 0.5).convert_linear_rgb()),
            reflectivity_color: None,
            transparency_mode: Some(match base.alpha_mode() {
                AlphaMode::Opaque => MaterialTransparencyMode::MtmOpaque,
                AlphaMode::Mask(_) => MaterialTransparencyMode::MtmAlphaTest,
                _ => MaterialTransparencyMode::MtmAlphaBlend,
            } as i32),
            metallic: Some(base.metallic),
            roughness: Some(base.perceptual_roughness),
            specular_intensity: None,
            emissive_intensity: None,
            direct_intensity: None,
        })
    }
}
