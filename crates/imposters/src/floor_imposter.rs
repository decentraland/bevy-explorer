use bevy::{
    asset::AssetLoader,
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::AsBindGroup,
};
use boimp::{
    asset_loader::ImposterLoader, bake::ImposterBakeMaterialExtension, render::Imposter,
    ImposterLoaderSettings,
};

#[derive(Clone, AsBindGroup, Asset, TypePath)]
pub struct FloorMaterialExt {
    #[uniform(100)]
    offset: f32,
}

impl MaterialExtension for FloorMaterialExt {
    fn vertex_shader() -> bevy::render::render_resource::ShaderRef {
        "embedded://shaders/floor_vertex.wgsl".into()
    }

    fn prepass_vertex_shader() -> bevy::render::render_resource::ShaderRef {
        "embedded://shaders/floor_vertex.wgsl".into()
    }

    fn fragment_shader() -> bevy::render::render_resource::ShaderRef {
        "embedded://shaders/floor_fragment.wgsl".into()
    }

    fn prepass_fragment_shader() -> bevy::render::render_resource::ShaderRef {
        "embedded://shaders/floor_fragment.wgsl".into()
    }
}

impl ImposterBakeMaterialExtension for FloorMaterialExt {
    fn imposter_fragment_shader() -> bevy::render::render_resource::ShaderRef {
        "embedded://shaders/floor_bake.wgsl".into()
    }
}

pub type FloorImposter = ExtendedMaterial<Imposter, FloorMaterialExt>;

#[derive(Default)]
pub struct FloorImposterLoader;

impl AssetLoader for FloorImposterLoader {
    type Asset = FloorImposter;
    type Settings = f32;
    type Error = anyhow::Error;

    async fn load(
        &self,
        reader: &mut dyn bevy::asset::io::Reader,
        settings: &Self::Settings,
        load_context: &mut bevy::asset::LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let base = ImposterLoader
            .load(
                reader,
                &ImposterLoaderSettings {
                    multisample: true,
                    alpha_blend: 0.5,
                    immediate_upload: true,
                    ..Default::default()
                },
                load_context,
            )
            .await?;
        Ok(FloorImposter {
            base,
            extension: FloorMaterialExt { offset: *settings },
        })
    }
}
