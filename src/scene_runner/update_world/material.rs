use bevy::{pbr::NotShadowCaster, prelude::*};

use crate::{
    dcl::interface::ComponentPosition,
    dcl_component::{
        proto_components::sdk::components::{pb_material, MaterialTransparencyMode, PbMaterial},
        SceneComponentId,
    },
    scene_runner::SceneSets,
};

use super::AddCrdtInterfaceExt;

pub struct MaterialDefinitionPlugin;

#[derive(Component, Debug, Default)]
pub struct MaterialDefinition {
    material: StandardMaterial,
    shadow_caster: bool,
}

impl From<PbMaterial> for MaterialDefinition {
    fn from(value: PbMaterial) -> Self {
        let material = match &value.material {
            Some(pb_material::Material::Unlit(unlit)) => {
                let base_color = unlit.diffuse_color.map(Color::from).unwrap_or(Color::WHITE);

                let alpha_mode = if base_color.a() < 1.0 {
                    AlphaMode::Blend
                } else {
                    AlphaMode::Opaque
                };

                StandardMaterial {
                    base_color,
                    double_sided: true,
                    cull_mode: None,
                    unlit: true,
                    alpha_mode,
                    ..Default::default()
                }
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
                }
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

fn update_materials(
    mut commands: Commands,
    new_materials: Query<(Entity, &MaterialDefinition), Changed<MaterialDefinition>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (ent, defn) in new_materials.iter() {
        // info!("found a mat for {ent:?}");
        let mut commands = commands.entity(ent);
        commands.insert(materials.add(defn.material.clone()));
        if defn.shadow_caster {
            commands.remove::<NotShadowCaster>();
        } else {
            commands.insert(NotShadowCaster);
        }
    }
}
