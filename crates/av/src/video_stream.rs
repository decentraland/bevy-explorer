use bevy::prelude::*;
use common::util::ReportErr;
use ipfs::IpfsResource;
use media::{AVCommand, VideoData};

use crate::{AVPlayer, AVSinks};
#[cfg(feature = "ffmpeg")]
use {
    crate::audio_sink::AudioSink,
    common::structs::AudioDecoderError,
    dcl_component::proto_components::sdk::components::VideoState,
    kira::sound::streaming::StreamingSoundData,
    media::{ffmpeg_worker, AudioHandle},
};

pub struct VideoSink {
    pub source: String,
    pub command_sender: tokio::sync::mpsc::UnboundedSender<AVCommand>,
    pub video_receiver: tokio::sync::mpsc::Receiver<VideoData>,
    pub image: Handle<Image>,
    pub current_time: f64,
    pub last_reported_time: f64,
    pub length: Option<f64>,
    pub rate: Option<f64>,
    /// The image changed size and the materials on it must rebind.
    pub resized: bool,
    /// The last volume sent, to send only changes.
    pub sent_volume: Option<f32>,
}

impl VideoSink {
    fn new(
        source: String,
        command_sender: tokio::sync::mpsc::UnboundedSender<AVCommand>,
        video_receiver: tokio::sync::mpsc::Receiver<VideoData>,
        image: Handle<Image>,
    ) -> Self {
        Self {
            source,
            command_sender,
            video_receiver,
            image,
            current_time: -1.0,
            last_reported_time: -1.0,
            length: None,
            rate: None,
            resized: false,
            sent_volume: None,
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

    #[cfg(feature = "ffmpeg")]
    let audio = {
        let (audio_sender, audio_receiver) = tokio::sync::mpsc::channel(10);
        let (handle_sender, handle_receiver) = tokio::sync::oneshot::channel();
        spawn_av_thread(
            ipfs,
            command_receiver,
            video_sender,
            audio_sender,
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
            Some(url),
            T::has_video(),
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
        video: Some(VideoSink::new(
            source,
            command_sender,
            video_receiver,
            image,
        )),
        _phantom: Default::default(),
    }
}

#[cfg(feature = "ffmpeg")]
pub fn spawn_av_thread(
    ipfs: IpfsResource,
    commands: tokio::sync::mpsc::UnboundedReceiver<AVCommand>,
    frames: tokio::sync::mpsc::Sender<VideoData>,
    audio: tokio::sync::mpsc::Sender<StreamingSoundData<AudioDecoderError>>,
    audio_handle: tokio::sync::oneshot::Receiver<AudioHandle>,
    path: String,
    hash: String,
) {
    std::thread::spawn(move || av_thread(ipfs, commands, frames, audio, audio_handle, path, hash));
}

#[cfg(feature = "ffmpeg")]
fn av_thread(
    ipfs: IpfsResource,
    commands: tokio::sync::mpsc::UnboundedReceiver<AVCommand>,
    frames: tokio::sync::mpsc::Sender<VideoData>,
    audio: tokio::sync::mpsc::Sender<StreamingSoundData<AudioDecoderError>>,
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
        audio_handle,
        path,
        hash,
    ) {
        frames
            .blocking_send(VideoData::State(VideoState::VsError))
            .report();
        warn!("av error: {e}");
    } else {
        debug!("av closed");
    }
}

/// Sinks that never play: an empty source.
pub fn noop_sinks<T: AVPlayer>(source: String, image: Handle<Image>) -> AVSinks<T> {
    let (command_sender, command_receiver) = tokio::sync::mpsc::unbounded_channel();
    let (video_sender, video_receiver) = tokio::sync::mpsc::channel(10);

    #[cfg(feature = "ffmpeg")]
    let audio = {
        let (_audio_sender, audio_receiver) = tokio::sync::mpsc::channel(10);
        let (handle_sender, _handle_receiver) = tokio::sync::oneshot::channel();
        drop(command_receiver);
        drop(video_sender);
        Some(AudioSink::new(audio_receiver, handle_sender))
    };
    #[cfg(feature = "html")]
    media::spawn_av(command_receiver, video_sender, None, T::has_video(), &image);

    AVSinks {
        #[cfg(feature = "ffmpeg")]
        audio,
        video: Some(VideoSink::new(
            source,
            command_sender,
            video_receiver,
            image,
        )),
        _phantom: Default::default(),
    }
}
