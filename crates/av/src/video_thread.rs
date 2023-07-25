use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use bevy::prelude::*;
use ffmpeg_next::ffi::AVPixelFormat;
use ffmpeg_next::format::{input, Pixel};
use ffmpeg_next::software::scaling::{context::Context, flag::Flags};
use ffmpeg_next::{decoder, format::context::Input, media::Type, util::frame, Packet};
use isahc::ReadResponseExt;
use thiserror::Error;
use tokio::sync::mpsc::error::TryRecvError;
pub enum AVCommand {
    Play,
    Pause,
    Repeat(bool),
    Seek(f64),
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
    Frame(frame::Video, f64),
}

#[derive(Component)]
pub struct VideoSink {
    pub source: String,
    pub command_sender: tokio::sync::mpsc::Sender<AVCommand>,
    pub data_receiver: tokio::sync::mpsc::Receiver<VideoData>,
    pub image: Handle<Image>,
    pub current_time: f64,
    pub length: Option<f64>,
    pub rate: Option<f64>,
}

impl VideoSink {
    pub fn new(source: String, image: Handle<Image>, playing: bool, repeat: bool) -> Self {
        let (command_sender, command_receiver) = tokio::sync::mpsc::channel(10);
        let (data_sender, data_receiver) = tokio::sync::mpsc::channel(10);

        spawn_av_thread(command_receiver, data_sender, source.clone());

        if playing {
            command_sender.blocking_send(AVCommand::Play).unwrap();
        }
        command_sender
            .blocking_send(AVCommand::Repeat(repeat))
            .unwrap();

        Self {
            source,
            command_sender,
            data_receiver,
            image,
            current_time: 0.0,
            length: None,
            rate: None,
        }
    }
}

pub fn spawn_av_thread(
    commands: tokio::sync::mpsc::Receiver<AVCommand>,
    frames: tokio::sync::mpsc::Sender<VideoData>,
    path: String,
) {
    std::thread::spawn(move || av_thread(commands, frames, path));
}

fn av_thread(
    commands: tokio::sync::mpsc::Receiver<AVCommand>,
    frames: tokio::sync::mpsc::Sender<VideoData>,
    path: String,
) {
    if let Err(e) = av_thread_inner(commands, frames, path) {
        warn!("av error: {e}");
    } else {
        debug!("av closed");
    }
}

trait FfmpegContext {
    fn stream_index(&self) -> Option<usize>;
    fn has_frame(&self) -> bool;
    fn receive_packet(&mut self, packet: Packet) -> Result<(), anyhow::Error>;
    fn is_dummy(&self) -> bool;
    fn send_frame(&mut self);
    fn set_start_frame(&mut self);
    fn reset_start_frame(&mut self);
    fn seconds_till_next_frame(&self) -> f64;
}

pub struct VideoContext {
    stream_index: usize,
    decoder: decoder::Video,
    scaler_context: Context,
    rate: f64,
    buffer: VecDeque<frame::video::Video>,
    sink: tokio::sync::mpsc::Sender<VideoData>,
    current_frame: usize,
    start_frame: usize,
}

#[derive(Debug, Error)]
pub enum AVContextError {
    #[error("Bad pixel format")]
    BadPixelFormat,
    #[error("No Stream")]
    NoStream,
    #[error("Remote channel closed")]
    ChannelClosed,
    #[error("Failed: {0}")]
    Failed(ffmpeg_next::Error),
}

impl VideoContext {
    fn init(
        input_context: &Input,
        sink: tokio::sync::mpsc::Sender<VideoData>,
    ) -> Result<Self, AVContextError> {
        let input_stream = input_context
            .streams()
            .best(Type::Video)
            .ok_or(AVContextError::NoStream)?;

        let pixel_format: AVPixelFormat =
            unsafe { std::mem::transmute((*input_stream.parameters().as_ptr()).format) };

        if pixel_format == AVPixelFormat::AV_PIX_FMT_NONE {
            return Err(AVContextError::BadPixelFormat);
        }

        let stream_index = input_stream.index();

        let context_decoder =
            ffmpeg_next::codec::context::Context::from_parameters(input_stream.parameters())
                .map_err(AVContextError::Failed)?;

        let decoder = context_decoder
            .decoder()
            .video()
            .map_err(AVContextError::Failed)?;

        let roundup = |x: u32| {
            (x.saturating_sub(1) / 8 + 1) * 8
            // x
        };

        let width = roundup(decoder.width());
        let height = roundup(decoder.height());

        let scaler_context = Context::get(
            decoder.format(),
            decoder.width(),
            decoder.height(),
            Pixel::RGBA,
            width,
            height,
            Flags::BILINEAR,
        )
        .map_err(AVContextError::Failed)?;

        let rate = f64::from(input_stream.rate());
        let length = (input_stream.frames() as f64) / rate;
        debug!("frames: {}, length: {}", input_stream.frames(), length);

        if sink
            .blocking_send(VideoData::Info(VideoInfo {
                width,
                height,
                rate,
                length,
            }))
            .is_err()
        {
            // channel closed
            return Err(AVContextError::ChannelClosed);
        }

        Ok(VideoContext {
            stream_index,
            rate,
            decoder,
            scaler_context,
            buffer: Default::default(),
            sink,
            current_frame: 0,
            start_frame: 0,
        })
    }
}

impl FfmpegContext for VideoContext {
    fn is_dummy(&self) -> bool {
        false
    }

    fn stream_index(&self) -> Option<usize> {
        Some(self.stream_index)
    }

    fn receive_packet(&mut self, packet: Packet) -> Result<(), anyhow::Error> {
        self.decoder.send_packet(&packet).unwrap();
        let mut decoded = frame::Video::empty();
        if let Ok(()) = self.decoder.receive_frame(&mut decoded) {
            let mut rgb_frame = frame::Video::empty();
            // run frame through scaler for color space conversion
            self.scaler_context.run(&decoded, &mut rgb_frame)?;
            self.buffer.push_back(rgb_frame);
        }
        Ok(())
    }

    fn has_frame(&self) -> bool {
        !self.buffer.is_empty()
    }

    fn send_frame(&mut self) {
        debug!("send video frame {:?}", self.current_frame);
        let _ = self.sink.blocking_send(VideoData::Frame(
            self.buffer.pop_front().unwrap(),
            self.current_frame as f64 / self.rate,
        ));
        self.current_frame += 1;
    }

    fn set_start_frame(&mut self) {
        self.start_frame = self.current_frame;
    }

    fn reset_start_frame(&mut self) {
        self.start_frame = 0;
    }

    fn seconds_till_next_frame(&self) -> f64 {
        (self.current_frame - self.start_frame) as f64 / self.rate
    }
}

pub struct DummyContext;

impl FfmpegContext for DummyContext {
    fn has_frame(&self) -> bool {
        true
    }

    fn receive_packet(&mut self, _: Packet) -> Result<(), anyhow::Error> {
        panic!()
    }

    fn is_dummy(&self) -> bool {
        true
    }

    fn stream_index(&self) -> Option<usize> {
        None
    }

    fn send_frame(&mut self) {}

    fn set_start_frame(&mut self) {}

    fn reset_start_frame(&mut self) {}

    fn seconds_till_next_frame(&self) -> f64 {
        f64::MAX
    }
}

pub fn av_thread_inner(
    mut commands: tokio::sync::mpsc::Receiver<AVCommand>,
    sink: tokio::sync::mpsc::Sender<VideoData>,
    path: String,
) -> Result<(), anyhow::Error> {
    let mut input_context = input(&path)?;

    // try and get a video context
    let video_context: Option<VideoContext> = {
        match VideoContext::init(&input_context, sink.clone()) {
            Ok(vc) => Some(vc),
            Err(AVContextError::BadPixelFormat) => {
                // try to workaround ffmpeg remote streaming issue by downloading the file
                debug!("failed to determine pixel format - downloading ...");
                let mut resp = isahc::get(&path)?;
                let data = resp.bytes()?;
                let local_folder = PathBuf::from("assets/video_downloads");
                std::fs::create_dir_all(&local_folder)?;
                let local_path = local_folder.join(Path::new(urlencoding::encode(&path).as_ref()));
                std::fs::write(&local_path, data)?;
                input_context = input(&local_path)?;
                Some(VideoContext::init(&input_context, sink).map_err(|e| anyhow::anyhow!(e))?)
            }
            Err(AVContextError::NoStream) => None,
            Err(AVContextError::Failed(ffmpeg_err)) => Err(ffmpeg_err)?,
            Err(AVContextError::ChannelClosed) => return Ok(()),
        }
    };

    // audio todo

    let mut contexts: Vec<Box<dyn FfmpegContext>> = Vec::default();
    if let Some(video_context) = video_context {
        contexts.push(Box::new(video_context) as Box<dyn FfmpegContext>);
    } else {
        contexts.push(Box::new(DummyContext) as Box<dyn FfmpegContext>);
    };

    if contexts.iter().all(|ctx| ctx.is_dummy()) {
        return Ok(());
    }

    let mut start_instant: Option<Instant> = None;
    let mut repeat = false;
    let mut stream_ended = false;

    loop {
        while !stream_ended && contexts.iter().any(|ctx| !ctx.has_frame()) {
            if let Some((stream, packet)) = input_context.packets().next() {
                for context in &mut contexts {
                    if Some(stream.index()) == context.stream_index() {
                        context.receive_packet(packet)?;
                        break; // for
                    }
                }
            } else {
                stream_ended = true;
            }
        }

        if stream_ended {
            // eof
            if repeat {
                if input_context.seek(0, ..).is_err() {
                    // reload
                    input_context = input(&path)?;
                }
                for context in &mut contexts {
                    context.reset_start_frame();
                }
                if start_instant.is_some() {
                    start_instant = Some(Instant::now());
                }
                stream_ended = false;
                continue;
            } else {
                info!("eof");
                start_instant = None;
            }
        }

        let cmd = if start_instant.is_some() {
            commands.try_recv()
        } else {
            commands.blocking_recv().ok_or(TryRecvError::Disconnected)
        };

        match cmd {
            Ok(AVCommand::Play) => {
                if start_instant.is_none() && !stream_ended {
                    start_instant = Some(Instant::now())
                }
            }
            Ok(AVCommand::Pause) => start_instant = None,
            Ok(AVCommand::Repeat(r)) => repeat = r,
            Ok(AVCommand::Seek(_time)) => {
                todo!();
                // tbd
            }
            Err(TryRecvError::Empty) => (),
            Err(TryRecvError::Disconnected) | Ok(AVCommand::Dispose) => return Ok(()),
        }

        if let Some(play_instant) = start_instant {
            let (next_index, next_frame_time) = contexts.iter().enumerate().fold(
                (None, f64::MAX),
                |(context_index, min), (ix, context)| {
                    let ctx_time = context.seconds_till_next_frame();
                    if ctx_time < min {
                        (Some(ix), ctx_time)
                    } else {
                        (context_index, min)
                    }
                },
            );
            let now = Instant::now();
            let next_frame_time = play_instant + Duration::from_secs_f64(next_frame_time);
            if let Some(sleep_time) = next_frame_time.checked_duration_since(now) {
                println!("zzz: {sleep_time:?}");
                std::thread::sleep(sleep_time);
            } else {
                // we lost time - reset start frame and instant
                for context in &mut contexts {
                    context.set_start_frame();
                }
                start_instant = Some(now);
            }

            if let Some(index) = next_index {
                let context = contexts.get_mut(index).unwrap();
                context.send_frame();
            }
        }
    }
}
