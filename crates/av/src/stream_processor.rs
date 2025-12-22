use anyhow::bail;
use bevy::{
    log::{info, trace},
    prelude::debug,
};
use dcl_component::proto_components::sdk::components::VideoState;
use ffmpeg_next::Packet;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::error::TryRecvError;

use crate::ffmpeg_util::{PacketIter, BUFFER_TIME};

#[derive(Debug)]
pub enum AVCommand {
    Play,
    Pause,
    Repeat(bool),
    Seek(f64),
    Dispose,
}

pub trait FfmpegContext {
    fn is_live(&self) -> bool;
    fn stream_index(&self) -> Option<usize>;
    fn has_frame(&self) -> bool;
    fn buffered_time(&self) -> f64;
    fn receive_packet(&mut self, packet: Packet) -> Result<(), anyhow::Error>;
    fn send_frame(&mut self);
    fn set_start_frame(&mut self);
    fn reset_start_frame(&mut self);
    fn seconds_till_next_frame(&self) -> f64;
    fn update_state(&self, state: VideoState);
    fn clear(&mut self);
}

// pumps packets through stream contexts keeping them in sync
pub fn process_streams(
    mut input_context: impl PacketIter,
    streams: &mut [&mut dyn FfmpegContext],
    mut commands: tokio::sync::mpsc::Receiver<AVCommand>,
) -> Result<(), anyhow::Error> {
    let mut start_instant: Option<Instant> = None;
    let mut repeat = false;
    let mut init = false;
    let mut last_state = VideoState::VsNone;

    let mut update_state = |state: VideoState, streams: &mut [&mut dyn FfmpegContext]| {
        if state != last_state {
            for stream in streams {
                stream.update_state(state)
            }
            last_state = state;
        }
    };

    let mut tick = 0;

    loop {
        // check if all receivers were dropped
        if streams.iter().all(|ctx| !ctx.is_live()) {
            bail!("all streams disconnected without dispose command");
        }

        // ensure frame available
        while !input_context.is_eof() && streams.iter().any(|ctx| ctx.buffered_time() == 0.0) {
            update_state(VideoState::VsBuffering, streams);

            if let Some((stream_index, packet)) = input_context.blocking_next() {
                for stream in streams.iter_mut() {
                    if Some(stream_index) == stream.stream_index() {
                        stream.receive_packet(packet)?;
                        break; // for
                    }
                }
            }
        }

        // state ready if required
        if !init {
            update_state(VideoState::VsReady, streams);
            init = true;
        }

        if input_context.is_eof() {
            // eof
            if repeat {
                input_context.reset();
                for stream in streams.iter_mut() {
                    stream.reset_start_frame();
                }
                continue;
            } else if streams.iter().all(|ctx| ctx.buffered_time() == 0.0) {
                info!("eof");
                for stream in streams.iter() {
                    info!("stream: {}", stream.buffered_time());
                }
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
                if start_instant.is_none() && !input_context.is_eof() {
                    start_instant = Some(Instant::now());
                    for stream in streams.iter_mut() {
                        stream.set_start_frame();
                    }
                }
            }
            Ok(AVCommand::Pause) => {
                start_instant = None;
            }
            Ok(AVCommand::Repeat(r)) => repeat = r,
            Ok(AVCommand::Seek(time)) => {
                for stream in streams.iter_mut() {
                    stream.clear();
                }
                input_context.seek_to(time);
                update_state(VideoState::VsSeeking, streams);
                continue;
            }
            Err(TryRecvError::Empty) => (),
            Err(TryRecvError::Disconnected) | Ok(AVCommand::Dispose) => return Ok(()),
        }

        if start_instant.is_some() {
            update_state(VideoState::VsPlaying, streams);
        } else {
            update_state(VideoState::VsPaused, streams);
        }

        if let Some(play_instant) = start_instant {
            let (next_index, next_frame_time) = streams.iter().enumerate().fold(
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

            if tick % 25 == 0 {
                trace!(
                    "[{:?}] next frame time: {next_frame_time:?}/ now: {now:?}",
                    std::thread::current().id()
                );
            }
            tick += 1;
            let buffer_till_time = next_frame_time - Duration::from_millis(10);
            // preload frames
            while streams.iter().any(|ctx| ctx.buffered_time() < BUFFER_TIME)
                && Instant::now() < buffer_till_time
            {
                if let Some((stream_index, packet)) = input_context.try_next() {
                    for stream in streams.iter_mut() {
                        if Some(stream_index) == stream.stream_index() {
                            stream.receive_packet(packet)?;
                            break; // for
                        }
                    }
                }
            }

            if let Some(sleep_time) = next_frame_time.checked_duration_since(Instant::now()) {
                std::thread::sleep(sleep_time);
            } else if let Some(lost_time) = Instant::now().checked_duration_since(next_frame_time) {
                if lost_time > Duration::from_secs(1) {
                    // we lost time - reset start frame and instant
                    debug!("reset on loss");
                    for stream in streams.iter_mut() {
                        stream.set_start_frame();
                    }
                    start_instant = Some(now);
                }
            }

            if let Some(index) = next_index {
                let context = streams.get_mut(index).unwrap();
                context.send_frame();
            }
        }
    }
}
