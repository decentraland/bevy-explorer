use anyhow::bail;
use bevy::log::{debug, info, trace};
use dcl_component::proto_components::sdk::components::VideoState;
use ffmpeg_next::Packet;
use kira::tween::Tween;
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};
use tokio::sync::mpsc::error::TryRecvError;

use crate::{
    AVCommand,
    ffmpeg::{
        AudioHandle,
        util::{BUFFER_TIME, PacketIter},
    },
};

/// The stream's audio output: the kira sound the engine spawns once the stream's sound data
/// reaches it, handed here so the backend applies `AVCommand::Volume` itself.
pub struct AudioOutput {
    handle: tokio::sync::oneshot::Receiver<AudioHandle>,
    sound: Option<AudioHandle>,
    volume: f32,
}

impl AudioOutput {
    pub fn new(handle: tokio::sync::oneshot::Receiver<AudioHandle>) -> Self {
        Self {
            handle,
            sound: None,
            volume: 1.0,
        }
    }

    fn poll(&mut self) {
        if self.sound.is_none()
            && let Ok(mut sound) = self.handle.try_recv()
        {
            sound.set_volume(self.volume as f64, Tween::default());
            self.sound = Some(sound);
        }
    }

    fn set_volume(&mut self, volume: f32) {
        self.volume = volume;
        if let Some(sound) = &mut self.sound {
            sound.set_volume(volume as f64, Tween::default());
        }
    }
}

impl Drop for AudioOutput {
    fn drop(&mut self) {
        if let Some(sound) = &mut self.sound {
            sound.stop(Tween::default());
        }
    }
}

pub trait FfmpegContext {
    fn is_live(&self) -> bool;
    fn stream_index(&self) -> Option<usize>;
    #[expect(unused)]
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
    mut commands: tokio::sync::mpsc::UnboundedReceiver<AVCommand>,
    mut audio: AudioOutput,
) -> Result<(), anyhow::Error> {
    let mut start_instant: Option<Instant> = None;
    let mut repeat = false;
    let mut init = false;
    let mut last_state = VideoState::VsNone;

    let mut update_state = |state: VideoState, streams: &mut [&mut dyn FfmpegContext]| {
        trace!("Setting state to {state:?}.");
        if state != last_state {
            for stream in streams {
                stream.update_state(state)
            }
            last_state = state;
        }
    };

    let mut tick = 0;
    // commands received while buffering, handled once buffering is done
    let mut deferred = VecDeque::new();

    loop {
        trace!("Process stream");
        audio.poll();
        // check if all receivers were dropped
        if streams.iter().all(|ctx| !ctx.is_live()) {
            bail!("all streams disconnected without dispose command");
        }

        // ensure frame available
        if !input_context.is_eof() && streams.iter().any(|ctx| ctx.buffered_time() == 0.0) {
            trace!("Buffering stream");
            update_state(VideoState::VsBuffering, streams);
            while !input_context.is_eof() && streams.iter().any(|ctx| ctx.buffered_time() == 0.0) {
                // stop if the player went away while we were waiting on the input
                if streams.iter().all(|ctx| !ctx.is_live()) {
                    trace!("Player dropped while buffering.");
                    return Ok(());
                }
                match commands.try_recv() {
                    Ok(AVCommand::Dispose) | Err(TryRecvError::Disconnected) => {
                        trace!("Disposed while buffering.");
                        return Ok(());
                    }
                    Ok(cmd) => deferred.push_back(cmd),
                    Err(TryRecvError::Empty) => (),
                }
                if let Some((stream_index, packet)) = input_context.blocking_next() {
                    for stream in streams.iter_mut() {
                        if Some(stream_index) == stream.stream_index() {
                            stream.receive_packet(packet)?;
                            break; // for
                        }
                    }
                }
            }
        }

        // state ready if required
        if !init {
            trace!("Init stream");
            update_state(VideoState::VsReady, streams);
            init = true;
        }

        if input_context.is_eof() {
            trace!("End of stream");
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

        let cmd = if let Some(cmd) = deferred.pop_front() {
            Ok(cmd)
        } else if start_instant.is_some() {
            commands.try_recv()
        } else {
            trace!("Blocking on command channel.");
            commands.blocking_recv().ok_or(TryRecvError::Disconnected)
        };

        match cmd {
            Ok(AVCommand::Play) => {
                trace!("Play command.");
                if start_instant.is_none() && !input_context.is_eof() {
                    start_instant = Some(Instant::now());
                    for stream in streams.iter_mut() {
                        stream.set_start_frame();
                    }
                }
            }
            Ok(AVCommand::Pause) => {
                trace!("Pause command.");
                start_instant = None;
            }
            Ok(AVCommand::Repeat(r)) => {
                trace!("Repeat command.");
                repeat = r;
            }
            Ok(AVCommand::Seek(time)) => {
                trace!("Seek command.");
                for stream in streams.iter_mut() {
                    stream.clear();
                }
                input_context.seek_to(time);
                update_state(VideoState::VsSeeking, streams);
                continue;
            }
            Ok(AVCommand::Volume(volume)) => {
                trace!("Volume command.");
                audio.set_volume(volume);
            }
            Ok(AVCommand::Dispose) => {
                trace!("Dispose stream.");
                return Ok(());
            }
            Err(TryRecvError::Empty) => {
                trace!("Empty command channel.");
            }
            Err(TryRecvError::Disconnected) => {
                trace!("Command channel disconnected.");
                return Ok(());
            }
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

            #[expect(clippy::collapsible_if)]
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

#[cfg(test)]
mod tests {
    use super::*;

    // input that never produces a packet and never reaches eof, like a source stuck on read errors
    struct StuckInput;

    impl PacketIter for StuckInput {
        fn is_eof(&self) -> bool {
            false
        }
        fn try_next(&mut self) -> Option<(usize, Packet)> {
            None
        }
        fn blocking_next(&mut self) -> Option<(usize, Packet)> {
            None
        }
        fn reset(&mut self) {}
        fn seek_to(&mut self, _time: f64) {}
    }

    struct EmptyStream;

    impl FfmpegContext for EmptyStream {
        fn is_live(&self) -> bool {
            true
        }
        fn stream_index(&self) -> Option<usize> {
            Some(0)
        }
        fn has_frame(&self) -> bool {
            false
        }
        fn buffered_time(&self) -> f64 {
            0.0
        }
        fn receive_packet(&mut self, _packet: Packet) -> Result<(), anyhow::Error> {
            Ok(())
        }
        fn send_frame(&mut self) {}
        fn set_start_frame(&mut self) {}
        fn reset_start_frame(&mut self) {}
        fn seconds_till_next_frame(&self) -> f64 {
            f64::MAX
        }
        fn update_state(&self, _state: VideoState) {}
        fn clear(&mut self) {}
    }

    #[test]
    fn buffering_exits_on_dispose_with_open_channel() {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        sender.send(AVCommand::Dispose).unwrap();
        let mut stream = EmptyStream;
        process_streams(
            StuckInput,
            &mut [&mut stream],
            receiver,
            AudioOutput::new(tokio::sync::oneshot::channel().1),
        )
        .unwrap();
        drop(sender);
    }

    #[test]
    fn buffering_exits_when_command_channel_closes() {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        drop(sender);
        let mut stream = EmptyStream;
        process_streams(
            StuckInput,
            &mut [&mut stream],
            receiver,
            AudioOutput::new(tokio::sync::oneshot::channel().1),
        )
        .unwrap();
    }
}
