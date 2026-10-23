use bevy::prelude::*;
use common::util::ReportErr;
use dcl_component::proto_components::sdk::components::VideoState;
use ipfs::IpfsResource;
use media::{AVCommand, VideoData};

use crate::{AVPlayer, AVSinks};
#[cfg(feature = "ffmpeg")]
use {
    crate::audio_sink::AudioSink,
    common::structs::AudioDecoderError,
    kira::sound::streaming::StreamingSoundData,
    media::{ffmpeg_worker, AudioHandle},
};

pub struct VideoSink {
    pub source: String,
    pub video_receiver: tokio::sync::mpsc::Receiver<VideoData>,
    pub image: Handle<Image>,
    pub current_time: f64,
    pub last_reported_time: f64,
    pub length: Option<f64>,
    /// The image changed size and the materials on it must rebind.
    pub resized: bool,
}

impl VideoSink {
    fn new(
        source: String,
        video_receiver: tokio::sync::mpsc::Receiver<VideoData>,
        image: Handle<Image>,
    ) -> Self {
        Self {
            source,
            video_receiver,
            image,
            current_time: -1.0,
            last_reported_time: -1.0,
            length: None,
            resized: false,
        }
    }
}

/// Opens `source` on the platform's av backend, rendering video into `image`. The audio and
/// video sinks share one command channel.
pub fn av_sinks<T: AVPlayer>(
    ipfs: IpfsResource,
    source: String,
    hash: String,
    image: Handle<Image>,
    playing: bool,
    repeat: bool,
) -> AVSinks<T> {
    let (command_sender, command_receiver) = tokio::sync::mpsc::unbounded_channel();
    let (video_sender, video_receiver) = tokio::sync::mpsc::channel(10);
    let (state_sender, state_receiver) = tokio::sync::watch::channel(VideoState::VsNone);

    #[cfg(feature = "ffmpeg")]
    let audio = {
        let (audio_sender, audio_receiver) = tokio::sync::mpsc::channel(10);
        let (handle_sender, handle_receiver) = tokio::sync::oneshot::channel();
        spawn_av_thread(
            ipfs,
            command_receiver,
            T::HAS_VIDEO.then_some(video_sender),
            audio_sender,
            state_sender,
            handle_receiver,
            source.clone(),
            hash,
        );
        Some(AudioSink::new(audio_receiver, handle_sender))
    };
    #[cfg(feature = "html")]
    {
        let url = ipfs
            .content_url(&source, &hash)
            .unwrap_or_else(|| source.clone());
        media::spawn_av(
            command_receiver,
            video_sender,
            state_sender,
            Some(url),
            T::HAS_VIDEO,
            &image,
        );
    }

    if playing {
        command_sender.send(AVCommand::Play).report();
    }
    command_sender.send(AVCommand::Repeat(repeat)).report();

    AVSinks {
        #[cfg(feature = "ffmpeg")]
        audio,
        video: T::HAS_VIDEO.then_some(VideoSink::new(source, video_receiver, image)),
        command_sender,
        rate: None,
        state: state_receiver,
        sent_volume: None,
        sent_state: VideoState::VsNone,
        _phantom: Default::default(),
    }
}

#[expect(clippy::too_many_arguments)]
#[cfg(feature = "ffmpeg")]
pub fn spawn_av_thread(
    ipfs: IpfsResource,
    commands: tokio::sync::mpsc::UnboundedReceiver<AVCommand>,
    frames: Option<tokio::sync::mpsc::Sender<VideoData>>,
    audio: tokio::sync::mpsc::Sender<StreamingSoundData<AudioDecoderError>>,
    state_sender: tokio::sync::watch::Sender<VideoState>,
    audio_handle: tokio::sync::oneshot::Receiver<AudioHandle>,
    path: String,
    hash: String,
) {
    std::thread::spawn(move || {
        av_thread(
            ipfs,
            commands,
            frames,
            audio,
            state_sender,
            audio_handle,
            path,
            hash,
        )
    });
}

#[expect(clippy::too_many_arguments)]
#[cfg(feature = "ffmpeg")]
fn av_thread(
    ipfs: IpfsResource,
    commands: tokio::sync::mpsc::UnboundedReceiver<AVCommand>,
    frames: Option<tokio::sync::mpsc::Sender<VideoData>>,
    audio: tokio::sync::mpsc::Sender<StreamingSoundData<AudioDecoderError>>,
    state_sender: tokio::sync::watch::Sender<VideoState>,
    audio_handle: tokio::sync::oneshot::Receiver<AudioHandle>,
    path: String,
    hash: String,
) {
    info!(
        "spawned av thread {:?}, path {path}",
        std::thread::current().id()
    );
    let _span = bevy::log::tracing::info_span!("av-thread").entered();
    if let Err(e) = ffmpeg_worker(
        &ipfs,
        commands,
        frames.clone(),
        audio,
        state_sender.clone(),
        audio_handle,
        path,
        hash,
    ) {
        state_sender.send(VideoState::VsError).report();
        warn!("av error: {e}");
    } else {
        debug!("av closed");
    }
}

/// Sinks that never play: an empty source.
pub fn noop_sinks<T: AVPlayer>(source: String, image: Handle<Image>) -> AVSinks<T> {
    let (command_sender, command_receiver) = tokio::sync::mpsc::unbounded_channel();
    let (video_sender, video_receiver) = tokio::sync::mpsc::channel(10);
    #[cfg_attr(not(feature = "html"), expect(unused))]
    let (state_sender, state_receiver) = tokio::sync::watch::channel(VideoState::VsNone);

    #[cfg(feature = "ffmpeg")]
    let audio = {
        let (_audio_sender, audio_receiver) = tokio::sync::mpsc::channel(10);
        let (handle_sender, _handle_receiver) = tokio::sync::oneshot::channel();
        drop(command_receiver);
        drop(video_sender);
        Some(AudioSink::new(audio_receiver, handle_sender))
    };
    #[cfg(feature = "html")]
    media::spawn_av(
        command_receiver,
        video_sender,
        state_sender,
        None,
        T::HAS_VIDEO,
        &image,
    );

    AVSinks {
        #[cfg(feature = "ffmpeg")]
        audio,
        video: Some(VideoSink::new(source, video_receiver, image)),
        command_sender,
        rate: None,
        state: state_receiver,
        sent_volume: None,
        sent_state: VideoState::VsNone,
        _phantom: Default::default(),
    }
}
