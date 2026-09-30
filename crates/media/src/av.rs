//! The av backend's interface, the same on every platform: a source is driven with
//! [`AVCommand`]s and reports [`VideoData`]. Native decodes with ffmpeg on a thread; the web
//! plays the source in a page media element ([`crate::html`]).

use dcl_component::proto_components::sdk::components::VideoState;

#[derive(Debug)]
pub enum AVCommand {
    Play,
    Pause,
    Repeat(bool),
    Seek(f64),
    /// Applied where the backend mixes: the page element on the web. Native mixes in kira on
    /// the engine side and ignores it.
    Volume(f32),
    Dispose,
}

pub struct VideoInfo {
    pub width: u32,
    pub height: u32,
    pub rate: f64,
    pub length: f64,
}

pub enum VideoData {
    Info(VideoInfo),
    Frame(VideoFrame, f64),
    State(VideoState),
}

/// A decoded frame. Native carries the pixels; on the web the page copied the frame straight to
/// the render worker, which writes it into the target image itself, so only the arrival is
/// reported.
pub struct VideoFrame {
    #[cfg(all(not(target_arch = "wasm32"), feature = "ffmpeg"))]
    pub(crate) frame: ffmpeg_next::frame::Video,
}

impl VideoFrame {
    /// The frame's rgba pixels, if they come through the engine.
    pub fn data(&self) -> Option<&[u8]> {
        #[cfg(all(not(target_arch = "wasm32"), feature = "ffmpeg"))]
        {
            Some(self.frame.data(0))
        }
        #[cfg(not(all(not(target_arch = "wasm32"), feature = "ffmpeg")))]
        {
            None
        }
    }
}
