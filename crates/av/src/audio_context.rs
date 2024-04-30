use std::{collections::VecDeque, time::Duration};

use bevy::prelude::*;
use common::structs::AudioDecoderError;
use dcl_component::proto_components::sdk::components::VideoState;
use ffmpeg_next::ffi::AVSampleFormat;
use ffmpeg_next::{decoder, format::context::Input, media::Type, util::frame, Packet};
use kira::sound::streaming::StreamingSoundData;
use thiserror::Error;
use tokio::sync::mpsc::error::TryRecvError;

use crate::stream_processor::FfmpegContext;

trait SampleFormatHelper {
    fn is_planar(&self) -> bool;
    fn bytes(&self) -> usize;
    fn to_f32(&self, data: &[u8]) -> f32;
}

impl SampleFormatHelper for AVSampleFormat {
    fn is_planar(&self) -> bool {
        match self {
            AVSampleFormat::AV_SAMPLE_FMT_NONE
            | AVSampleFormat::AV_SAMPLE_FMT_U8
            | AVSampleFormat::AV_SAMPLE_FMT_S16
            | AVSampleFormat::AV_SAMPLE_FMT_S32
            | AVSampleFormat::AV_SAMPLE_FMT_FLT
            | AVSampleFormat::AV_SAMPLE_FMT_DBL
            | AVSampleFormat::AV_SAMPLE_FMT_NB => false,
            AVSampleFormat::AV_SAMPLE_FMT_U8P
            | AVSampleFormat::AV_SAMPLE_FMT_S16P
            | AVSampleFormat::AV_SAMPLE_FMT_S32P
            | AVSampleFormat::AV_SAMPLE_FMT_FLTP
            | AVSampleFormat::AV_SAMPLE_FMT_DBLP
            | AVSampleFormat::AV_SAMPLE_FMT_S64
            | AVSampleFormat::AV_SAMPLE_FMT_S64P => true,
        }
    }

    fn bytes(&self) -> usize {
        match self {
            AVSampleFormat::AV_SAMPLE_FMT_NONE => panic!(),
            AVSampleFormat::AV_SAMPLE_FMT_U8 => 1,
            AVSampleFormat::AV_SAMPLE_FMT_S16 => 2,
            AVSampleFormat::AV_SAMPLE_FMT_S32 => 4,
            AVSampleFormat::AV_SAMPLE_FMT_FLT => 4,
            AVSampleFormat::AV_SAMPLE_FMT_DBL => 8,
            AVSampleFormat::AV_SAMPLE_FMT_U8P => 1,
            AVSampleFormat::AV_SAMPLE_FMT_S16P => 2,
            AVSampleFormat::AV_SAMPLE_FMT_S32P => 4,
            AVSampleFormat::AV_SAMPLE_FMT_FLTP => 4,
            AVSampleFormat::AV_SAMPLE_FMT_DBLP => 8,
            AVSampleFormat::AV_SAMPLE_FMT_S64 => 8,
            AVSampleFormat::AV_SAMPLE_FMT_S64P => 8,
            AVSampleFormat::AV_SAMPLE_FMT_NB => panic!(),
        }
    }

    // TODO lots of optimization potential. branch per read?!
    fn to_f32(&self, data: &[u8]) -> f32 {
        assert!(data.len() == self.bytes());

        match self {
            AVSampleFormat::AV_SAMPLE_FMT_U8 | AVSampleFormat::AV_SAMPLE_FMT_U8P => {
                (data[0] as f32 / 255.0 - 0.5) * 2.0
            }
            AVSampleFormat::AV_SAMPLE_FMT_S16 | AVSampleFormat::AV_SAMPLE_FMT_S16P => unsafe {
                std::slice::from_raw_parts(data.as_ptr() as *const i16, 1)[0] as f32
                    / i16::MAX as f32
            },
            AVSampleFormat::AV_SAMPLE_FMT_S32 | AVSampleFormat::AV_SAMPLE_FMT_S32P => unsafe {
                std::slice::from_raw_parts(data.as_ptr() as *const i32, 1)[0] as f32
                    / i32::MAX as f32
            },
            AVSampleFormat::AV_SAMPLE_FMT_FLT | AVSampleFormat::AV_SAMPLE_FMT_FLTP => unsafe {
                std::slice::from_raw_parts(data.as_ptr() as *const f32, 1)[0]
            },
            AVSampleFormat::AV_SAMPLE_FMT_DBL | AVSampleFormat::AV_SAMPLE_FMT_DBLP => unsafe {
                std::slice::from_raw_parts(data.as_ptr() as *const f64, 1)[0] as f32
            },
            AVSampleFormat::AV_SAMPLE_FMT_S64 | AVSampleFormat::AV_SAMPLE_FMT_S64P => unsafe {
                std::slice::from_raw_parts(data.as_ptr() as *const i64, 1)[0] as f32
                    / i64::MAX as f32
            },
            AVSampleFormat::AV_SAMPLE_FMT_NONE | AVSampleFormat::AV_SAMPLE_FMT_NB => panic!(),
        }
    }
}

pub struct FfmpegKiraBridge {
    sample_rate: u32,
    num_frames: usize,
    frame_time: f64,
    frame_size: usize,
    format: AVSampleFormat,
    channels: usize,
    data: tokio::sync::mpsc::Receiver<ffmpeg_next::frame::Audio>,
    step: usize,
}

impl kira::sound::streaming::Decoder for FfmpegKiraBridge {
    type Error = AudioDecoderError;

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn num_frames(&self) -> usize {
        if self.num_frames == 0 {
            u32::MAX as usize
        } else {
            self.num_frames * self.frame_size
        }
    }

    fn decode(&mut self) -> Result<Vec<kira::dsp::Frame>, Self::Error> {
        self.step += 1;
        let mut frames = Vec::default();
        let bytes_per_sample = self.format.bytes();
        let is_planar = self.format.is_planar();
        loop {
            match self.data.try_recv() {
                Ok(frame) => {
                    if is_planar && self.channels > 1 {
                        let d0 = frame.data(0);
                        let d1 = frame.data(1);
                        frames.extend(
                            d0.chunks_exact(bytes_per_sample)
                                .take(self.frame_size)
                                .zip(d1.chunks_exact(bytes_per_sample))
                                .map(|(l, r)| kira::dsp::Frame {
                                    left: self.format.to_f32(l),
                                    right: self.format.to_f32(r),
                                }),
                        );
                    } else if self.channels == 1 {
                        frames.extend(
                            frame
                                .data(0)
                                .chunks_exact(bytes_per_sample)
                                .take(self.frame_size)
                                .map(|c| {
                                    let val = self.format.to_f32(c);
                                    kira::dsp::Frame {
                                        left: val,
                                        right: val,
                                    }
                                }),
                        );
                    } else {
                        frames.extend(
                            frame
                                .data(0)
                                .chunks_exact(bytes_per_sample * self.channels)
                                .take(self.frame_size)
                                .map(|c| kira::dsp::Frame {
                                    left: self.format.to_f32(&c[0..bytes_per_sample]),
                                    right: self
                                        .format
                                        .to_f32(&c[bytes_per_sample..bytes_per_sample * 2]),
                                }),
                        );
                    }
                }
                Err(TryRecvError::Empty) => {
                    // we must sleep here to stop the decoder thread from running constantly, else
                    // it will keep polling until it's frame buffer is full. as we are streaming
                    // our audio in realtime that will never happen.
                    // alternative: we could block until we have enough data to fill the buffer.
                    // that is unconfigurable and is 16k frames, so at 44100 would be ~1/3 second...
                    // maybe worth trying, but it just sleeps for 1ms anyway. we can do better by sleeping
                    // until a new frame arrives (typically 50ms+).
                    if frames.is_empty() {
                        debug!(
                            "[{:?} waiting for frames [step {}], sleep {}s",
                            std::thread::current().id(),
                            self.step,
                            self.frame_time.max(0.1)
                        );
                        std::thread::sleep(Duration::from_secs_f64(self.frame_time.max(0.1)));
                    } else {
                        return Ok(frames);
                    }
                }
                Err(TryRecvError::Disconnected) => {
                    if frames.is_empty() {
                        return Err(AudioDecoderError::StreamClosed);
                    } else {
                        return Ok(frames);
                    }
                }
            }
        }
    }

    fn seek(&mut self, _: usize) -> Result<usize, Self::Error> {
        Err(AudioDecoderError::Other("Can't seek".to_owned()))
    }
}

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("No Stream")]
    NoStream,
    #[error("Failed: {0}")]
    Failed(ffmpeg_next::Error),
}

pub struct AudioContext {
    stream_index: usize,
    decoder: decoder::Audio,

    rate: f64,

    buffer: VecDeque<frame::audio::Audio>,
    sink: tokio::sync::mpsc::Sender<ffmpeg_next::frame::Audio>,

    current_frame: usize,
    start_frame: usize,
    dead: bool,
}

impl AudioContext {
    pub fn init(
        input_context: &Input,
        channel: tokio::sync::mpsc::Sender<StreamingSoundData<AudioDecoderError>>,
    ) -> Result<Self, AudioError> {
        let input_stream = input_context
            .streams()
            .best(Type::Audio)
            .ok_or(AudioError::NoStream)?;

        let stream_index = input_stream.index();

        let context_decoder =
            ffmpeg_next::codec::context::Context::from_parameters(input_stream.parameters())
                .map_err(AudioError::Failed)?;

        let decoder = context_decoder
            .decoder()
            .audio()
            .map_err(AudioError::Failed)?;
        debug!(
            "decoder says: bitrate: {}, frame_rate: {:?}, channels: {}, frame_size: {}, channels: {}",
            decoder.bit_rate(),
            decoder.frame_rate(),
            decoder.channels(),
            decoder.frame_size(),
            decoder.channels()
        );

        let p = input_stream.parameters();
        let p_raw_sample_rate = unsafe { (*p.as_ptr()).sample_rate } as u32;
        let format: AVSampleFormat = unsafe { std::mem::transmute((*p.as_ptr()).format) };

        let frame_rate = p_raw_sample_rate as f64 / decoder.frame_size() as f64;
        let length = (input_stream.frames() as f64) / frame_rate;
        let length = if length.is_nan() { 1e6 } else { length };
        debug!(
            "frame_rate: {}, frames: {}, length: {}, format: {format:?}",
            frame_rate,
            input_stream.frames(),
            length
        );

        let (sx, rx) = tokio::sync::mpsc::channel(10);

        let kira_decoder = FfmpegKiraBridge {
            sample_rate: p_raw_sample_rate,
            num_frames: input_stream.frames() as usize,
            frame_size: decoder.frame_size() as usize,
            frame_time: frame_rate.recip(),
            format,
            channels: decoder.channels() as usize,
            data: rx,
            step: 0,
        };

        let sound_data = kira::sound::streaming::StreamingSoundData::from_decoder(
            kira_decoder,
            kira::sound::streaming::StreamingSoundSettings::new(),
        );

        let _ = channel.blocking_send(sound_data);

        Ok(AudioContext {
            stream_index,
            decoder,
            buffer: VecDeque::default(),
            sink: sx,
            current_frame: 0,
            start_frame: 0,
            rate: frame_rate,
            dead: false,
        })
    }
}

impl FfmpegContext for AudioContext {
    fn is_live(&self) -> bool {
        !self.sink.is_closed()
    }

    fn stream_index(&self) -> Option<usize> {
        Some(self.stream_index)
    }

    fn receive_packet(&mut self, packet: Packet) -> Result<(), anyhow::Error> {
        self.decoder.send_packet(&packet).unwrap();
        let mut decoded = frame::Audio::empty();
        if let Ok(()) = self.decoder.receive_frame(&mut decoded) {
            self.buffer.push_back(decoded);
        }
        Ok(())
    }

    fn has_frame(&self) -> bool {
        !self.buffer.is_empty()
    }

    fn buffered_time(&self) -> f64 {
        self.buffer.len() as f64 / self.rate
    }

    fn send_frame(&mut self) {
        if !self.dead {
            debug!(
                "send audio frame {:?} [{} in buffer]",
                self.current_frame,
                self.buffer.len()
            );
            if let Err(e) = self.sink.blocking_send(self.buffer.pop_front().unwrap()) {
                error!("failed to send audio frame: {e}");
                self.dead = true;
            }
        } else {
            self.buffer.pop_front();
        }
        self.current_frame += 1;
    }

    fn set_start_frame(&mut self) {
        self.start_frame = self.current_frame;
    }

    fn reset_start_frame(&mut self) {}

    fn seconds_till_next_frame(&self) -> f64 {
        (self.current_frame - self.start_frame + 1) as f64 / self.rate
    }

    fn update_state(&self, _state: VideoState) {
        // disregard
    }

    fn clear(&mut self) {
        self.buffer.clear();
        self.current_frame = 0;
        self.start_frame = 0;
    }
}
