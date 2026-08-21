mod audio_context;
mod stream_processor;
#[cfg(test)]
mod test;
mod util;
mod video_context;

use std::path::Path;

use bevy::log::{debug, trace};
use common::{structs::AudioDecoderError, util::ReportErr};
use dcl_component::proto_components::sdk::components::VideoState;
use ffmpeg_next::format::input;
use ipfs::IpfsIo;
use kira::sound::streaming::StreamingSoundData;

use crate::ffmpeg::{
    audio_context::{AudioContext, AudioError},
    stream_processor::process_streams,
    util::InputWrapper,
    video_context::{VideoContext, VideoError},
};

pub use {
    ffmpeg_next::frame::Video,
    stream_processor::AVCommand,
    video_context::{VideoData, VideoInfo},
};

pub fn init_ffmpeg() {
    ffmpeg_next::init().unwrap();
    ffmpeg_next::log::set_level(ffmpeg_next::log::Level::Error);
}

pub fn ffmpeg_worker(
    ipfas: &IpfsIo,
    commands: tokio::sync::mpsc::UnboundedReceiver<AVCommand>,
    video: tokio::sync::mpsc::Sender<VideoData>,
    audio: tokio::sync::mpsc::Sender<StreamingSoundData<AudioDecoderError>>,
    mut path: String,
    hash: String,
) -> Result<(), anyhow::Error> {
    video
        .blocking_send(VideoData::State(VideoState::VsLoading))
        .report();
    debug!("av thread spawned for {path} ...");
    let download = |url: &str| -> Result<String, anyhow::Error> {
        let local_folder = ipfas.cache_path().unwrap().join("video_downloads");
        let local_path = local_folder.join(Path::new(urlencoding::encode(url).as_ref()));

        if std::fs::File::open(&local_path).is_err() {
            let resp = reqwest::blocking::get(url)?;
            let data = resp.bytes()?;
            std::fs::create_dir_all(&local_folder)?;
            std::fs::write(&local_path, data)?;
        }
        Ok(local_path.to_string_lossy().to_string())
    };

    // source might be a content map file or a url
    if let Some(content_url) = ipfas.content_url(&path, &hash) {
        // check if it changed as content_url will return Some(path) when not found and path is url-compliant.
        // if it is a raw url we don't want to download initially as some servers reject http get requests on videos.
        if content_url != path {
            // for content paths we download
            debug!(
                "content map file {} -> {}, downloading ...",
                path, content_url
            );
            path = download(&content_url)?;
        }
    };

    let mut input_context = input(&path)?;

    // try and get a video context
    let video_context: Option<VideoContext> = {
        match VideoContext::init(&input_context, video.clone()) {
            Ok(vc) => Some(vc),
            Err(VideoError::BadPixelFormat) => {
                // try to workaround ffmpeg remote streaming issue by downloading the file
                debug!("failed to determine pixel format - downloading ...");
                let path = download(&path)?;
                input_context = input(&path)?;
                Some(VideoContext::init(&input_context, video).map_err(|e| anyhow::anyhow!(e))?)
            }
            Err(VideoError::NoStream) => None,
            Err(VideoError::Failed(ffmpeg_err)) => Err(ffmpeg_err)?,
            Err(VideoError::ChannelClosed) => return Ok(()),
        }
    };

    // try and get an audio context
    let audio_context: Option<AudioContext> = match AudioContext::init(&input_context, audio) {
        Ok(ac) => Some(ac),
        Err(AudioError::NoStream) => None,
        Err(AudioError::Failed(ffmpeg_err)) => Err(ffmpeg_err)?,
    };

    if video_context.is_none() && audio_context.is_none() {
        debug!("No data for video from path {path}.");
    }

    let input_context = InputWrapper::new(input_context, path);

    match (video_context, audio_context) {
        (None, None) => Ok(()),
        (None, Some(mut ac)) => {
            trace!("Processing stream with audio only");
            process_streams(input_context, &mut [&mut ac], commands)
        }
        (Some(mut vc), None) => {
            trace!("Processing stream with video only");
            process_streams(input_context, &mut [&mut vc], commands)
        }
        (Some(mut vc), Some(mut ac)) => {
            trace!("Processing stream");
            process_streams(input_context, &mut [&mut vc, &mut ac], commands)
        }
    }
}
