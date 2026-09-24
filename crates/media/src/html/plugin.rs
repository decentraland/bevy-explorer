use std::cell::RefCell;

use bevy::{
    platform::collections::HashMap,
    prelude::*,
    render::{
        Render, RenderApp, RenderSet,
        render_asset::{RenderAssets, prepare_assets},
        render_resource::{Extent3d, TextureUsages},
        renderer::{RenderDevice, RenderQueue},
        texture::GpuImage,
    },
};
use futures_channel::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::error::TryRecvError;
use wasm_bindgen::{JsCast, closure::Closure};
use web_sys::VideoFrame;

use super::{
    AvSource, HOST, HostCommand, MediaEvent, MediaId, MediaTarget, NEW_SOURCES, TARGETS, send_host,
    send_target,
};
use crate::{AVCommand, VideoData, VideoFrame as AvFrame};

pub struct HtmlMediaPlugin;

impl Plugin for HtmlMediaPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MediaEventReceiver>()
            .init_resource::<AvSources>()
            .add_systems(PreUpdate, serve_av_sources);

        let (sx, rx) = tokio::sync::mpsc::unbounded_channel();
        let _ = TARGETS.set(sx);
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .insert_resource(MediaTargetReceiver(rx))
                .init_resource::<MediaTargets>()
                .add_systems(
                    Render,
                    // before the gpu images are (re)prepared, so a material rebind the same
                    // frame binds the texture the copy resized
                    perform_video_copies
                        .in_set(RenderSet::PrepareAssets)
                        .before(prepare_assets::<GpuImage>),
                );
        } else {
            error!("No RenderApp, HtmlMedia will not work.");
        }
    }

    /// Runs on the render worker: listens for the frames the page transfers here.
    fn finish(&self, _app: &mut App) {
        let Ok(global) = js_sys::global().dyn_into::<web_sys::DedicatedWorkerGlobalScope>() else {
            return;
        };
        let on_message =
            Closure::<dyn FnMut(web_sys::MessageEvent)>::new(|event: web_sys::MessageEvent| {
                let data = event.data();
                let is_frame = js_sys::Reflect::get(&data, &"type".into())
                    .ok()
                    .and_then(|t| t.as_string())
                    .is_some_and(|t| t == "VIDEO_FRAME");
                if !is_frame {
                    return;
                }
                let id = js_sys::Reflect::get(&data, &"id".into())
                    .ok()
                    .and_then(|id| id.as_f64())
                    .unwrap_or_default() as MediaId;
                if let Ok(frame) = js_sys::Reflect::get(&data, &"frame".into())
                    .and_then(|frame| frame.dyn_into::<VideoFrame>())
                {
                    media_render_frame(id, frame);
                }
            });
        if let Err(e) =
            global.add_event_listener_with_callback("message", on_message.as_ref().unchecked_ref())
        {
            error!("html media: cannot listen for video frames: {e:?}");
        }
        on_message.forget();
    }
}

/// The page's event receiver, taken once the page has installed the host.
#[derive(Resource, Default)]
struct MediaEventReceiver(Option<UnboundedReceiver<MediaEvent>>);

#[derive(Resource, Default)]
struct AvSources(HashMap<MediaId, AvSource>);

/// Relays the open sources' commands to the page and the page's events back as [`VideoData`].
fn serve_av_sources(mut sources: ResMut<AvSources>, mut receiver: ResMut<MediaEventReceiver>) {
    for source in NEW_SOURCES.lock().unwrap().drain(..) {
        sources.0.insert(source.id, source);
    }

    sources.0.retain(|id, source| {
        loop {
            match source.commands.try_recv() {
                Ok(AVCommand::Dispose) | Err(TryRecvError::Disconnected) => {
                    send_host(HostCommand::Drop(*id));
                    send_target(*id, None);
                    break false;
                }
                Ok(command) => send_host(HostCommand::Command(*id, command)),
                Err(TryRecvError::Empty) => break true,
            }
        }
    });

    if receiver.0.is_none() {
        receiver.0 = HOST
            .get()
            .and_then(|host| host.events.lock().unwrap().take());
    }
    let Some(receiver) = receiver.0.as_mut() else {
        return;
    };
    while let Ok(event) = receiver.try_recv() {
        match event {
            MediaEvent::State {
                id,
                state,
                duration,
            } => {
                if let Some(source) = sources.0.get_mut(&id) {
                    if source.duration != duration {
                        source.duration = duration;
                        source.send_info();
                    }
                    source.send(VideoData::State(state));
                }
            }
            MediaEvent::Frame {
                id,
                media_time,
                width,
                height,
            } => {
                if let Some(source) = sources.0.get_mut(&id) {
                    if source.size != Some((width, height)) {
                        source.size = Some((width, height));
                        source.send_info();
                    }
                    source.send(VideoData::Frame(AvFrame {}, media_time));
                }
            }
        }
    }
}

// Render worker side.

thread_local! {
    /// Frames the page transferred to this worker, newest last.
    static FRAMES: RefCell<Vec<(MediaId, VideoFrame)>> = const { RefCell::new(Vec::new()) };
}

/// A `VIDEO_FRAME` message the page posted to the render worker.
fn media_render_frame(id: MediaId, frame: VideoFrame) {
    FRAMES.with(|frames| frames.borrow_mut().push((id, frame)));
}

#[derive(Resource)]
struct MediaTargetReceiver(tokio::sync::mpsc::UnboundedReceiver<MediaTarget>);

#[derive(Resource, Default)]
struct MediaTargets(HashMap<MediaId, AssetId<Image>>);

fn perform_video_copies(
    mut target_updates: ResMut<MediaTargetReceiver>,
    mut targets: ResMut<MediaTargets>,
    mut images: ResMut<RenderAssets<GpuImage>>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
) {
    while let Ok(MediaTarget { id, target }) = target_updates.0.try_recv() {
        match target {
            Some(target) => {
                targets.0.insert(id, target);
            }
            None => {
                targets.0.remove(&id);
            }
        }
    }

    // Only the newest frame per media is worth copying.
    let mut latest: HashMap<MediaId, VideoFrame> = HashMap::default();
    for (id, frame) in FRAMES.with(|frames| std::mem::take(&mut *frames.borrow_mut())) {
        if let Some(prev) = latest.insert(id, frame) {
            prev.close();
        }
    }

    for (id, frame) in latest.drain() {
        let Some(gpu_image) = targets
            .0
            .get(&id)
            .and_then(|target| images.get_mut(*target))
        else {
            debug!("media {id}: no target image yet");
            frame.close();
            continue;
        };
        let visible_rect = frame.visible_rect().unwrap();
        let size = Extent3d {
            width: visible_rect.width() as u32,
            height: visible_rect.height() as u32,
            depth_or_array_layers: 1,
        };

        if gpu_image.size != size {
            // Size the image to the frame here; the main world's image only follows suit
            // (`VideoData::Info`), it never re-uploads a video image.
            debug!("media {id}: resize {:?} -> {size:?}", gpu_image.size);
            let mut descriptor = gpu_image.texture_descriptor.clone();
            descriptor.size = size;
            descriptor.usage |= TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
            let texture = render_device.create_texture(&descriptor);
            gpu_image.texture_view = match &gpu_image.texture_view_descriptor {
                Some(view_descriptor) => texture.create_view(view_descriptor),
                None => texture.create_view(&Default::default()),
            };
            gpu_image.texture = texture;
            gpu_image.texture_descriptor = descriptor;
            gpu_image.size = size;
        }

        trace!(
            "media {id} -> {:?} perform {size:?}",
            gpu_image.texture_view
        );

        // `VideoFrame::clone` is WebCodecs' (a new frame); this is the JS handle.
        let frame_handle = Clone::clone(&frame);
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
            size,
        );

        frame_handle.close();
    }
}
