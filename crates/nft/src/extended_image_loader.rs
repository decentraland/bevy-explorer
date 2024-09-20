use bevy::{
    asset::{AssetLoader, AsyncReadExt},
    log::debug,
    prelude::{FromWorld, Image},
    render::texture::{
        CompressedImageFormats, ImageLoader, ImageLoaderError, ImageLoaderSettings, ImageType,
    },
    tasks::futures_lite::AsyncSeekExt,
};

pub struct SvgLoader;

impl AssetLoader for SvgLoader {
    type Asset = Image;
    type Settings = ImageLoaderSettings;
    type Error = &'static str;

    fn load<'a>(
        &'a self,
        reader: &'a mut bevy::asset::io::Reader,
        settings: &'a Self::Settings,
        _load_context: &'a mut bevy::asset::LoadContext,
    ) -> impl bevy::utils::ConditionalSendFuture<Output = Result<Self::Asset, Self::Error>> {
        Box::pin(async move {
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
            let img = Image::from_buffer(
                &png,
                ImageType::Extension("png"),
                CompressedImageFormats::default(),
                true,
                settings.sampler.clone(),
                settings.asset_usage,
            )
            .map_err(|_| "image construction failed")?;

            debug!("svg load ok");
            Ok(img)
        })
    }
}

pub struct ExtendedImageLoader {
    image_loader: ImageLoader,
    svg_loader: SvgLoader,
}

impl FromWorld for ExtendedImageLoader {
    fn from_world(world: &mut bevy::prelude::World) -> Self {
        Self {
            image_loader: ImageLoader::from_world(world),
            svg_loader: SvgLoader,
        }
    }
}

impl AssetLoader for ExtendedImageLoader {
    type Asset = Image;
    type Settings = ImageLoaderSettings;
    type Error = ImageLoaderError;

    fn load<'a>(
        &'a self,
        reader: &'a mut bevy::asset::io::Reader,
        settings: &'a Self::Settings,
        load_context: &'a mut bevy::asset::LoadContext,
    ) -> impl bevy::utils::ConditionalSendFuture<Output = Result<Self::Asset, Self::Error>> {
        Box::pin(async move {
            match self.image_loader.load(reader, settings, load_context).await {
                Ok(img) => Ok(img),
                Err(ImageLoaderError::FileTexture(e)) => {
                    if load_context
                        .path()
                        .to_str()
                        .map_or(false, |p| p.to_ascii_lowercase().contains("svg"))
                    {
                        // try svg
                        reader.seek(std::io::SeekFrom::Start(0)).await?;
                        self.svg_loader
                            .load(reader, settings, load_context)
                            .await
                            .map_err(|e| {
                                ImageLoaderError::Io(std::io::Error::new(
                                    std::io::ErrorKind::Other,
                                    e,
                                ))
                            })
                    } else {
                        Err(ImageLoaderError::FileTexture(e))
                    }
                }
                Err(other) => Err(other),
            }
        })
    }

    fn extensions(&self) -> &[&str] {
        &["image"]
    }
}
