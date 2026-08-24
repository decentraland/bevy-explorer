use bevy::prelude::*;
use common::{structs::AudioDecoderError, util::ReportErr};
use dcl_component::proto_components::sdk::components::VideoState;
use ipfs::IpfsResource;
use kira::sound::streaming::StreamingSoundData;
use media::{ffmpeg_worker, AVCommand, VideoData};

use crate::audio_sink::AudioSink;

pub struct VideoSink {
    pub source: String,
    pub command_sender: tokio::sync::mpsc::UnboundedSender<AVCommand>,
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
    let (command_sender, command_receiver) = tokio::sync::mpsc::unbounded_channel();
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
        command_sender.send(AVCommand::Play).report();
    }
    command_sender.send(AVCommand::Repeat(repeat)).report();

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
    commands: tokio::sync::mpsc::UnboundedReceiver<AVCommand>,
    frames: tokio::sync::mpsc::Sender<VideoData>,
    audio: tokio::sync::mpsc::Sender<StreamingSoundData<AudioDecoderError>>,
    path: String,
    hash: String,
) {
    std::thread::spawn(move || av_thread(ipfs, commands, frames, audio, path, hash));
}

fn av_thread(
    ipfs: IpfsResource,
    commands: tokio::sync::mpsc::UnboundedReceiver<AVCommand>,
    frames: tokio::sync::mpsc::Sender<VideoData>,
    audio: tokio::sync::mpsc::Sender<StreamingSoundData<AudioDecoderError>>,
    path: String,
    hash: String,
) {
    info!(
        "spawned av thread {:?}, path {path}",
        std::thread::current().id()
    );
    let _span = bevy::log::tracing::info_span!("av-thread").entered();
    if let Err(e) = ffmpeg_worker(&ipfs, commands, frames.clone(), audio, path, hash) {
        frames
            .blocking_send(VideoData::State(VideoState::VsError))
            .report();
        warn!("av error: {e}");
    } else {
        debug!("av closed");
    }
}

pub fn noop_sinks(source: String, image: Handle<Image>, volume: f32) -> (VideoSink, AudioSink) {
    let (command_sender, _command_receiver) = tokio::sync::mpsc::unbounded_channel();
    let (_video_sender, video_receiver) = tokio::sync::mpsc::channel(10);
    let (_audio_sender, audio_receiver) = tokio::sync::mpsc::channel(10);

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
