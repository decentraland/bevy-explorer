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
        "shaders/floor_vertex.wgsl".into()
    }

    fn prepass_vertex_shader() -> bevy::render::render_resource::ShaderRef {
        "shaders/floor_vertex.wgsl".into()
    }

    fn fragment_shader() -> bevy::render::render_resource::ShaderRef {
        "shaders/floor_fragment.wgsl".into()
    }

    fn prepass_fragment_shader() -> bevy::render::render_resource::ShaderRef {
        "shaders/floor_fragment.wgsl".into()
    }
}

impl ImposterBakeMaterialExtension for FloorMaterialExt {
    fn imposter_fragment_shader() -> bevy::render::render_resource::ShaderRef {
        "shaders/floor_bake.wgsl".into()
    }
}

pub type FloorImposter = ExtendedMaterial<Imposter, FloorMaterialExt>;

#[derive(Default)]
pub struct FloorImposterLoader;

impl AssetLoader for FloorImposterLoader {
    type Asset = FloorImposter;
    type Settings = f32;
    type Error = anyhow::Error;

    fn load<'a>(
        &'a self,
        reader: &'a mut bevy::asset::io::Reader,
        settings: &'a Self::Settings,
        load_context: &'a mut bevy::asset::LoadContext,
    ) -> impl bevy::utils::ConditionalSendFuture<Output = Result<Self::Asset, Self::Error>> {
        Box::pin(async move {
            let base = ImposterLoader
                .load(
                    reader,
                    &ImposterLoaderSettings {
                        multisample: true,
                        alpha_blend: 0.5,
                        ..Default::default()
                    },
                    load_context,
                )
                .await?;
            Ok(FloorImposter {
                base,
                extension: FloorMaterialExt { offset: *settings },
            })
        })
    }
}
