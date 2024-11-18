use bevy::{
    asset::AssetLoader,
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::AsBindGroup,
};
use boimp::{asset_loader::ImposterLoader, render::Imposter, ImposterLoaderSettings};

#[derive(Clone, AsBindGroup, Asset, TypePath)]
pub struct FloorMaterialExt {}

impl MaterialExtension for FloorMaterialExt {
    fn vertex_shader() -> bevy::render::render_resource::ShaderRef {
        "shaders/floor_vertex.wgsl".into()
    }

    fn fragment_shader() -> bevy::render::render_resource::ShaderRef {
        "shaders/floor_fragment.wgsl".into()
    }
}

pub type FloorImposter = ExtendedMaterial<Imposter, FloorMaterialExt>;

#[derive(Default)]
pub struct FloorImposterLoader;

impl AssetLoader for FloorImposterLoader {
    type Asset = FloorImposter;
    type Settings = ();
    type Error = anyhow::Error;

    fn load<'a>(
        &'a self,
        reader: &'a mut bevy::asset::io::Reader,
        _: &'a Self::Settings,
        load_context: &'a mut bevy::asset::LoadContext,
    ) -> impl bevy::utils::ConditionalSendFuture<Output = Result<Self::Asset, Self::Error>> {
        Box::pin(async move {
            let base = ImposterLoader
                .load(
                    reader,
                    &ImposterLoaderSettings {
                        multisample: true,
                        ..Default::default()
                    },
                    load_context,
                )
                .await?;
            Ok(FloorImposter {
                base,
                extension: FloorMaterialExt {},
            })
        })
    }
}
