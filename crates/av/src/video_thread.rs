use std::time::{Duration, Instant};

use bevy::prelude::*;
use ffmpeg_next::frame::Video;
use ffmpeg_next::media::Type;
use ffmpeg_next::software::scaling::{context::Context, flag::Flags};
use ffmpeg_next::{
    ffi::AV_TIME_BASE,
    format::{input, Pixel},
};
use tokio::sync::mpsc::error::TryRecvError;

pub enum VideoCommand {
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
    Frame(Video, f64),
}

#[derive(Component)]
pub struct VideoSink {
    pub source: String,
    pub command_sender: tokio::sync::mpsc::Sender<VideoCommand>,
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

        spawn_video_thread(command_receiver, data_sender, source.clone());

        if playing {
            command_sender.blocking_send(VideoCommand::Play).unwrap();
        }
        command_sender
            .blocking_send(VideoCommand::Repeat(repeat))
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

pub fn spawn_video_thread(
    commands: tokio::sync::mpsc::Receiver<VideoCommand>,
    frames: tokio::sync::mpsc::Sender<VideoData>,
    path: String,
) {
    std::thread::spawn(move || video_thread(commands, frames, path));
}

fn video_thread(
    commands: tokio::sync::mpsc::Receiver<VideoCommand>,
    frames: tokio::sync::mpsc::Sender<VideoData>,
    path: String,
) {
    if let Err(e) = video_thread_inner(commands, frames, path) {
        warn!("video error: {e}");
    } else {
        warn!("video closed");
    }
}

pub fn video_thread_inner(
    mut commands: tokio::sync::mpsc::Receiver<VideoCommand>,
    sink: tokio::sync::mpsc::Sender<VideoData>,
    path: String,
) -> Result<(), anyhow::Error> {
    let mut input_context = input(&path)?;

    // initialize decoder
    let input_stream = input_context
        .streams()
        .best(Type::Video)
        .ok_or(ffmpeg_next::Error::StreamNotFound)?;

    let video_stream_index = input_stream.index();

    let context_decoder =
        ffmpeg_next::codec::context::Context::from_parameters(input_stream.parameters())?;

    let mut decoder = context_decoder.decoder().video()?;

    let roundup = |x: u32| {
        (x.saturating_sub(1) / 8 + 1) * 8
        // x
    };

    // initialize scaler
    let mut scaler_context = Context::get(
        decoder.format(),
        decoder.width(),
        decoder.height(),
        Pixel::RGBA,
        roundup(decoder.width()),
        roundup(decoder.height()),
        Flags::BILINEAR,
    )?;

    let rate = f64::from(input_stream.rate());
    let length = (input_stream.frames() as f64) / rate;
    println!("frames: {}, length: {}", input_stream.frames(), length);

    if let Err(_) = sink.blocking_send(VideoData::Info(VideoInfo {
        width: roundup(decoder.width()),
        height: roundup(decoder.height()),
        rate,
        length,
    })) {
        return Ok(());
    }

    let mut start_frame = 0;
    let mut current_frame = 0;
    let mut start_instant: Option<Instant> = None;
    let mut repeat = false;
    let mut next_frame = None;

    loop {
        if next_frame.is_none() {
            while let Some((stream, packet)) = input_context.packets().next() {
                // check if packets is for the selected video stream
                if stream.index() == video_stream_index {
                    decoder.send_packet(&packet).unwrap();
                    let mut decoded = Video::empty();
                    if let Ok(()) = decoder.receive_frame(&mut decoded) {
                        let mut rgb_frame = Video::empty();
                        // run frame through scaler for color space conversion
                        scaler_context.run(&decoded, &mut rgb_frame)?;
                        next_frame = Some(rgb_frame);
                        break;
                    }
                }
            }
        }

        if next_frame.is_none() {
            // eof
            if repeat {
                if let Err(_) = input_context.seek(0, ..) {
                    // reload
                    input_context = input(&path)?;
                }
                start_frame = 0;
                current_frame = 0;
                if start_instant.is_some() {
                    start_instant = Some(Instant::now());
                }
                continue;
            } else {
                start_instant = None;
            }
        }

        let cmd = if start_instant.is_some() {
            commands.try_recv()
        } else {
            commands.blocking_recv().ok_or(TryRecvError::Disconnected)
        };

        match cmd {
            Ok(VideoCommand::Play) => {
                if start_instant.is_none() {
                    start_instant = Some(Instant::now())
                }
            }
            Ok(VideoCommand::Pause) => start_instant = None,
            Ok(VideoCommand::Repeat(r)) => repeat = r,
            Ok(VideoCommand::Seek(time)) => {
                println!("seek -> {time}");
                let time = time.clamp(0.0, length);
                let av_time = (time * i64::from(AV_TIME_BASE) as f64) as i64;
                input_context.seek(av_time, 0..)?;
                let frame = (time * rate) as i64;
                start_frame = frame;
                current_frame = frame;
                if start_instant.is_some() {
                    start_instant = Some(Instant::now());
                }
            }
            Err(TryRecvError::Empty) => (),
            Err(TryRecvError::Disconnected) | Ok(VideoCommand::Dispose) => return Ok(()),
        }

        if let Some(play_instant) = start_instant {
            let now = Instant::now();
            let next_frame_time = play_instant
                + Duration::from_secs_f64((current_frame - start_frame + 1) as f64 / rate);
            if let Some(sleep_time) = next_frame_time.checked_duration_since(now) {
                std::thread::sleep(sleep_time);
            } else {
                // we lost time - reset start frame and instant
                start_frame = current_frame + 1;
                start_instant = Some(now);
            }

            current_frame += 1;
            println!("send frame {current_frame}");
            let _ = sink.blocking_send(VideoData::Frame(
                next_frame.take().unwrap(),
                current_frame as f64 / rate,
            ));
        }
    }
}
