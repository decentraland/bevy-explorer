use std::path::Path;

use bevy::{prelude::*, utils::tracing};
use common::structs::AudioDecoderError;
use dcl_component::proto_components::sdk::components::VideoState;
use ffmpeg_next::format::input;
use ipfs::{IpfsIo, IpfsResource};
use isahc::ReadResponseExt;
use kira::sound::streaming::StreamingSoundData;

use crate::{
    audio_context::{AudioContext, AudioError},
    audio_sink::AudioSink,
    ffmpeg_util::InputWrapper,
    stream_processor::{process_streams, AVCommand},
    video_context::{VideoContext, VideoData, VideoError},
};

#[derive(Component)]
pub struct VideoSink {
    pub source: String,
    pub command_sender: tokio::sync::mpsc::Sender<AVCommand>,
    pub video_receiver: tokio::sync::mpsc::Receiver<VideoData>,
    pub image: Handle<Image>,
    pub current_time: f64,
    pub last_reported_time: f64,
    pub length: Option<f64>,
    pub rate: Option<f64>,
}

pub fn av_sinks(
    ipfs: IpfsResource,
    source: String,
    hash: String,
    image: Handle<Image>,
    volume: f32,
    playing: bool,
    repeat: bool,
) -> (VideoSink, AudioSink) {
    let (command_sender, command_receiver) = tokio::sync::mpsc::channel(10);
    let (video_sender, video_receiver) = tokio::sync::mpsc::channel(10);
    let (audio_sender, audio_receiver) = tokio::sync::mpsc::channel(10);

    spawn_av_thread(
        ipfs,
        command_receiver,
        video_sender,
        audio_sender,
        source.clone(),
        hash,
    );

    if playing {
        command_sender.blocking_send(AVCommand::Play).unwrap();
    }
    command_sender
        .blocking_send(AVCommand::Repeat(repeat))
        .unwrap();

    (
        VideoSink {
            source,
            command_sender: command_sender.clone(),
            video_receiver,
            image,
            current_time: -1.0,
            last_reported_time: -1.0,
            length: None,
            rate: None,
        },
        AudioSink::new(volume, command_sender, audio_receiver),
    )
}

pub fn spawn_av_thread(
    ipfs: IpfsResource,
    commands: tokio::sync::mpsc::Receiver<AVCommand>,
    frames: tokio::sync::mpsc::Sender<VideoData>,
    audio: tokio::sync::mpsc::Sender<StreamingSoundData<AudioDecoderError>>,
    path: String,
    hash: String,
) {
    std::thread::spawn(move || av_thread(ipfs, commands, frames, audio, path, hash));
}

fn av_thread(
    ipfs: IpfsResource,
    commands: tokio::sync::mpsc::Receiver<AVCommand>,
    frames: tokio::sync::mpsc::Sender<VideoData>,
    audio: tokio::sync::mpsc::Sender<StreamingSoundData<AudioDecoderError>>,
    path: String,
    hash: String,
) {
    info!(
        "spawned av thread {:?}, path {path}",
        std::thread::current().id()
    );
    let _span = tracing::info_span!("av-thread").entered();
    if let Err(e) = av_thread_inner(&ipfs, commands, frames.clone(), audio, path, hash) {
        let _ = frames.blocking_send(VideoData::State(VideoState::VsError));
        warn!("av error: {e}");
    } else {
        debug!("av closed");
    }
}

pub fn av_thread_inner(
    ipfas: &IpfsIo,
    commands: tokio::sync::mpsc::Receiver<AVCommand>,
    video: tokio::sync::mpsc::Sender<VideoData>,
    audio: tokio::sync::mpsc::Sender<StreamingSoundData<AudioDecoderError>>,
    mut path: String,
    hash: String,
) -> Result<(), anyhow::Error> {
    let _ = video.blocking_send(VideoData::State(VideoState::VsLoading));
    debug!("av thread spawned for {path} ...");
    let download = |url: &str| -> Result<String, anyhow::Error> {
        let local_folder = ipfas.cache_path().join("video_downloads");
        let local_path = local_folder.join(Path::new(urlencoding::encode(url).as_ref()));

        if std::fs::File::open(&local_path).is_err() {
            let mut resp = isahc::get(url)?;
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
        // no data
    }

    let input_context = InputWrapper::new(input_context, path);

    match (video_context, audio_context) {
        (None, None) => Ok(()),
        (None, Some(mut ac)) => process_streams(input_context, &mut [&mut ac], commands),
        (Some(mut vc), None) => process_streams(input_context, &mut [&mut vc], commands),
        (Some(mut vc), Some(mut ac)) => {
            process_streams(input_context, &mut [&mut vc, &mut ac], commands)
        }
    }
}
