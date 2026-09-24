//! Html media on the web: the page owns the `<video>` elements (the engine runs on a worker,
//! which has no DOM) and plays the engine's av sources in them ([`host`]). The engine drives
//! them with the [`AVCommand`]s and [`VideoData`] native's ffmpeg thread takes and reports
//! ([`spawn_av`]); commands go engine → page and state comes back over `futures_channel`
//! (lock-free: the page must never block). Decoded frames never touch the engine: the page
//! transfers each `VideoFrame` straight to the render worker, tagged with the media id, and the
//! render world copies it into the media's target image, sized to the frame
//! ([`plugin::HtmlMediaPlugin`]); the engine only learns the frame's time and size.

pub mod host;
pub mod plugin;

use std::sync::{
    Mutex,
    atomic::{AtomicU32, Ordering},
};

use bevy::prelude::*;
use dcl_component::proto_components::sdk::components::VideoState;
use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender};
use once_cell::sync::OnceCell;

use crate::{AVCommand, VideoData, VideoInfo};

pub type MediaId = u32;

/// Engine → page.
#[derive(Debug)]
pub enum HostCommand {
    /// A `<video>` element for `url`; `None` creates one without a source (a placeholder that
    /// never plays). Only with `has_video` are its frames read (which needs a CORS load).
    Create {
        id: MediaId,
        url: Option<String>,
        has_video: bool,
    },
    Command(MediaId, AVCommand),
    Drop(MediaId),
}

/// Page → engine.
#[derive(Debug, Clone, Copy)]
pub enum MediaEvent {
    State {
        id: MediaId,
        state: VideoState,
        duration: f64,
    },
    /// A frame was transferred to the render worker.
    Frame {
        id: MediaId,
        media_time: f64,
        width: u32,
        height: u32,
    },
}

/// Engine → render worker: which image a media's frames are copied into.
pub struct MediaTarget {
    pub id: MediaId,
    pub target: Option<AssetId<Image>>,
}

/// Set by the page ([`host::media_host_main`]) before the engine starts.
struct HostHandle {
    commands: UnboundedSender<HostCommand>,
    events: Mutex<Option<UnboundedReceiver<MediaEvent>>>,
}

static HOST: OnceCell<HostHandle> = OnceCell::new();
/// Set by the engine's [`plugin::HtmlMediaPlugin`].
static TARGETS: OnceCell<tokio::sync::mpsc::UnboundedSender<MediaTarget>> = OnceCell::new();
static NEXT_ID: AtomicU32 = AtomicU32::new(1);
/// Sources opened since the plugin last looked.
static NEW_SOURCES: Mutex<Vec<AvSource>> = Mutex::new(Vec::new());

fn send_host(command: HostCommand) {
    match HOST.get() {
        Some(host) => {
            let _ = host.commands.unbounded_send(command);
        }
        None => debug!("no media host: dropping {command:?}"),
    }
}

fn send_target(id: MediaId, target: Option<AssetId<Image>>) {
    if let Some(targets) = TARGETS.get() {
        let _ = targets.send(MediaTarget { id, target });
    }
}

/// A source the engine opened: its commands are relayed to the page and the page's events
/// reported as [`VideoData`] by the plugin.
struct AvSource {
    id: MediaId,
    commands: tokio::sync::mpsc::UnboundedReceiver<AVCommand>,
    video: tokio::sync::mpsc::Sender<VideoData>,
    size: Option<(u32, u32)>,
    duration: f64,
}

impl AvSource {
    fn new(
        commands: tokio::sync::mpsc::UnboundedReceiver<AVCommand>,
        video: tokio::sync::mpsc::Sender<VideoData>,
        image: &Handle<Image>,
    ) -> Self {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        send_target(id, Some(image.id()));
        Self {
            id,
            commands,
            video,
            size: None,
            duration: 0.0,
        }
    }

    fn send(&self, data: VideoData) {
        if self.video.try_send(data).is_err() {
            trace!("media {}: video channel full or closed", self.id);
        }
    }

    fn send_info(&self) {
        if let Some((width, height)) = self.size {
            self.send(VideoData::Info(VideoInfo {
                width,
                height,
                rate: 0.0,
                length: self.duration,
            }));
        }
    }
}

/// Plays `url` in a page element whose frames are copied into `image`: the web counterpart of
/// native's ffmpeg thread, driven by `commands` and reporting on `video`. `None` opens a
/// placeholder that never plays. Without `has_video` (an audio stream) no frames are read.
pub fn spawn_av(
    commands: tokio::sync::mpsc::UnboundedReceiver<AVCommand>,
    video: tokio::sync::mpsc::Sender<VideoData>,
    url: Option<String>,
    has_video: bool,
    image: &Handle<Image>,
) {
    let source = AvSource::new(commands, video, image);
    send_host(HostCommand::Create {
        id: source.id,
        url,
        has_video,
    });
    NEW_SOURCES.lock().unwrap().push(source);
}

/// A page `<video>` element the page registers itself (a livekit track's) under the returned
/// id with [`host::adopt_video_element`]; its frames flow into `image` like any other
/// source's. Dropping `commands`' sender releases it.
pub fn adopt_video(
    commands: tokio::sync::mpsc::UnboundedReceiver<AVCommand>,
    video: tokio::sync::mpsc::Sender<VideoData>,
    image: &Handle<Image>,
) -> MediaId {
    let source = AvSource::new(commands, video, image);
    let id = source.id;
    NEW_SOURCES.lock().unwrap().push(source);
    id
}
