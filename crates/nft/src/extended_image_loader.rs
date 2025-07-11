use bevy::{
    asset::{io::VecReader, AssetLoader},
    image::{
        CompressedImageFormats, ImageLoader, ImageLoaderError, ImageLoaderSettings, ImageType,
    },
    log::debug,
    prelude::{FromWorld, Image},
    render::renderer::RenderDevice,
};

pub struct SvgLoader;

impl AssetLoader for SvgLoader {
    type Asset = Image;
    type Settings = ImageLoaderSettings;
    type Error = &'static str;

    async fn load(
        &self,
        reader: &mut dyn bevy::asset::io::Reader,
        settings: &Self::Settings,
        _load_context: &mut bevy::asset::LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| "read failed")?;
        debug!("svg reader read {} bytes", bytes.len());
        let svg_tree = resvg::usvg::Tree::from_data(&bytes, &Default::default())
            .map_err(|_| "tree build failed")?;
        let transform = resvg::tiny_skia::Transform::from_scale(
            512.0 / svg_tree.size().width(),
            512.0 / svg_tree.size().height(),
        );
        let Some(mut pixmap) = resvg::tiny_skia::Pixmap::new(512, 512) else {
            return Err("pixmap failed");
        };
        resvg::render(&svg_tree, transform, &mut pixmap.as_mut());
        let png = pixmap.encode_png().map_err(|_| "encode png failed")?;
        #[allow(unused_mut)]
        let mut image = Image::from_buffer(
            &png,
            ImageType::Extension("png"),
            CompressedImageFormats::default(),
            true,
            settings.sampler.clone(),
            settings.asset_usage,
        )
        .map_err(|_| "image construction failed")?;

        #[cfg(target_arch = "wasm32")]
        if image.texture_descriptor.format
            == bevy::render::render_resource::TextureFormat::Rgba16Unorm
        {
            let data = image
                .data
                .unwrap()
                .chunks_exact(2)
                .map(|pair| {
                    (u16::from_le_bytes([pair[0], pair[1]]) as f32 / u16::MAX as f32
                        * u8::MAX as f32) as u8
                })
                .collect::<Vec<_>>();
            image = Image::new(
                image.texture_descriptor.size,
                image.texture_descriptor.dimension,
                data,
                bevy::render::render_resource::TextureFormat::Rgba8Unorm,
                image.asset_usage,
            );
        }

        debug!("svg load ok");
        Ok(image)
    }
}

pub struct ExtendedImageLoader {
    image_loader: ImageLoader,
    svg_loader: SvgLoader,
}

impl FromWorld for ExtendedImageLoader {
    fn from_world(world: &mut bevy::prelude::World) -> Self {
        let supported_compressed_formats = match world.get_resource::<RenderDevice>() {
            Some(render_device) => CompressedImageFormats::from_features(render_device.features()),
            None => CompressedImageFormats::NONE,
        };

        Self {
            image_loader: ImageLoader::new(supported_compressed_formats),
            svg_loader: SvgLoader,
        }
    }
}

impl AssetLoader for ExtendedImageLoader {
    type Asset = Image;
    type Settings = ImageLoaderSettings;
    type Error = ImageLoaderError;

    async fn load(
        &self,
        reader: &mut dyn bevy::asset::io::Reader,
        settings: &Self::Settings,
        load_context: &mut bevy::asset::LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut data = Vec::default();
        reader.read_to_end(&mut data).await?;

        match self
            .image_loader
            .load(&mut VecReader::new(data.clone()), settings, load_context)
            .await
        {
            Ok(img) => Ok(img),
            Err(ImageLoaderError::FileTexture(e)) => {
                if load_context
                    .path()
                    .to_str()
                    .is_some_and(|p| p.to_ascii_lowercase().contains("svg"))
                {
                    // try svg
                    self.svg_loader
                        .load(&mut VecReader::new(data), settings, load_context)
                        .await
                        .map_err(|e| ImageLoaderError::Io(std::io::Error::other(e)))
                } else {
                    Err(ImageLoaderError::FileTexture(e))
                }
            }
            Err(other) => Err(other),
        }
    }

    fn extensions(&self) -> &[&str] {
        &["image"]
    }
}
