use bevy::{
    platform::collections::HashMap,
    prelude::*,
    render::{
        Render, RenderApp, RenderSet, render_asset::RenderAssets, render_resource::Extent3d,
        renderer::RenderQueue, texture::GpuImage,
    },
};

use super::{FrameCopyRequest, FrameCopyRequestQueue};

pub struct HtmlMediaPlugin;

impl Plugin for HtmlMediaPlugin {
    fn build(&self, app: &mut App) {
        let (sx, rx) = tokio::sync::mpsc::unbounded_channel();
        app.insert_resource(FrameCopyRequestQueue(sx));

        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .insert_resource(FrameCopyReceiveQueue(rx))
                .add_systems(Render, perform_video_copies.in_set(RenderSet::Queue));
        } else {
            error!("No RenderApp, HtmlMedia will not work.");
        }
    }
}

#[derive(Resource)]
struct FrameCopyReceiveQueue(tokio::sync::mpsc::UnboundedReceiver<FrameCopyRequest>);

fn perform_video_copies(
    mut requests: ResMut<FrameCopyReceiveQueue>,
    images: Res<RenderAssets<GpuImage>>,
    render_queue: Res<RenderQueue>,
) {
    let mut latest_requests: HashMap<AssetId<Image>, FrameCopyRequest> = HashMap::new();

    while let Ok(request) = requests.0.try_recv() {
        if let Some(prev) = latest_requests.get(&request.target) {
            prev.video_frame.close();
        }
        latest_requests.insert(request.target, request);
    }

    for (_, request) in latest_requests.drain() {
        let frame_copy = request.video_frame.clone();
        let Some(gpu_image) = images.get(request.target) else {
            warn!("missing gpu image");
            continue;
        };
        let frame = request.video_frame.into_inner();
        let visible_rect = frame.visible_rect().unwrap();
        let source_size = (visible_rect.width() as u32, visible_rect.height() as u32);
        let target_size = (gpu_image.size.width, gpu_image.size.height);

        if source_size != target_size {
            warn!("skip frame {source_size:?} != {target_size:?}");
            continue;
        }

        trace!(
            "{:?}/{:?} perform {:?} -> {:?}",
            request.target, gpu_image.texture_view, source_size, target_size
        );

        render_queue.copy_external_image_to_texture(
            &wgpu::CopyExternalImageSourceInfo {
                source: wgpu::ExternalImageSource::VideoFrame(frame),
                origin: wgpu::Origin2d {
                    x: visible_rect.x() as u32,
                    y: visible_rect.y() as u32,
                },
                flip_y: false,
            },
            wgpu::CopyExternalImageDestInfo {
                texture: &gpu_image.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
                premultiplied_alpha: false, // Video frames are not typically premultiplied.
                color_space: wgpu::PredefinedColorSpace::Srgb,
            },
            Extent3d {
                width: source_size.0,
                height: source_size.1,
                depth_or_array_layers: 1,
            },
        );

        frame_copy.close();
    }
}
