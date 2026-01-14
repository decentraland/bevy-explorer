use std::time::Duration;

use bevy::prelude::*;
use common::structs::AudioDecoderError;
use futures_lite::{future::yield_now, StreamExt};
use livekit::{
    prelude::RemoteTrackPublication,
    track::{RemoteAudioTrack, RemoteVideoTrack},
    webrtc::{
        audio_stream::native::NativeAudioStream,
        native::yuv_helper,
        prelude::{AudioFrame, I420Buffer, RtcTrackState, VideoBuffer},
        video_stream::native::NativeVideoStream,
    },
};
use tokio::sync::mpsc;

pub struct AudioTrackKiraBridge {
    sample_rate: u32,
    receiver: mpsc::Receiver<AudioFrame<'static>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl AudioTrackKiraBridge {
    pub fn new(audio_track: RemoteAudioTrack, sample_rate: u32) -> Self {
        let sid = audio_track.sid();
        let rtc_stream = NativeAudioStream::new(audio_track.rtc_track(), sample_rate as i32, 1);

        let (sender, receiver) = mpsc::channel(480);
        std::thread::Builder::new()
            .name(sid.to_string())
            .spawn(move || {
                debug!(
                    "Audio worker thread {:?} ({:?}) started.",
                    std::thread::current().name().unwrap(),
                    std::thread::current().id()
                );

                let mut rtc_stream = rtc_stream;
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .thread_name(sid)
                    .enable_all()
                    .build()
                    .unwrap();

                let handle = runtime.spawn(async move {
                    while let Some(frame) = rtc_stream.next().await {
                        match sender.send(frame).await {
                            Ok(()) => (),
                            Err(mpsc::error::SendError(_)) => {
                                error!("Failed to send audio frame.");
                                break;
                            }
                        }
                    }
                });

                let rtc_track = audio_track.rtc_track();
                while rtc_track.state() == RtcTrackState::Live {
                    std::thread::sleep(Duration::from_millis(100));
                    runtime.block_on(yield_now());
                }
                handle.abort();

                debug!(
                    "Audio worker thread {:?} ({:?}) ended.",
                    std::thread::current().name().unwrap(),
                    std::thread::current().id()
                );
            })
            .unwrap();

        Self {
            sample_rate,
            receiver,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl kira::sound::streaming::Decoder for AudioTrackKiraBridge {
    type Error = AudioDecoderError;

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn num_frames(&self) -> usize {
        u32::MAX as usize
    }

    fn decode(&mut self) -> Result<Vec<kira::Frame>, Self::Error> {
        let mut frames = Vec::default();

        loop {
            match self.receiver.try_recv() {
                Ok(frame) => {
                    if frame.sample_rate != self.sample_rate {
                        warn!(
                            "sample rate changed?! was {}, now {}",
                            self.sample_rate, frame.sample_rate
                        );
                    }

                    if frame.num_channels != 1 {
                        warn!("frame has {} channels", frame.num_channels);
                    }

                    for i in 0..frame.samples_per_channel as usize {
                        let sample = frame.data[i] as f32 / i16::MAX as f32;
                        frames.push(kira::Frame::new(sample, sample));
                    }
                }
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    return Err(AudioDecoderError::StreamClosed)
                }
            }
        }
        Ok(frames)
    }

    fn seek(&mut self, seek: usize) -> Result<usize, Self::Error> {
        if seek == 0 {
            return Ok(0);
        }
        Err(AudioDecoderError::Other(format!(
            "Can't seek (requested {seek})"
        )))
    }
}

pub trait I420BufferExt {
    fn rgba_data(&self) -> Vec<u8>;
}

impl I420BufferExt for I420Buffer {
    fn rgba_data(&self) -> Vec<u8> {
        let width = self.width();
        let height = self.height();
        let stride = width * 4;

        let (stride_y, stride_u, stride_v) = self.strides();
        let (data_y, data_u, data_v) = self.data();

        let mut dst = vec![0; (width * height * 4) as usize];

        yuv_helper::i420_to_abgr(
            data_y,
            stride_y,
            data_u,
            stride_u,
            data_v,
            stride_v,
            &mut dst,
            stride,
            width as i32,
            height as i32,
        );

        dst
    }
}

pub async fn livekit_video_thread(
    video: RemoteVideoTrack,
    publication: RemoteTrackPublication,
    sender: mpsc::Sender<I420Buffer>,
) {
    let mut stream = NativeVideoStream::new(video.rtc_track());

    while let Some(frame) = stream.next().await {
        let buffer = frame.buffer.to_i420();
        if let Err(err) = sender.send(buffer).await {
            error!("Livekit video thread failed to send frame buffer due to '{err}'.");
            break;
        }
    }

    warn!("video track {:?} ended, exiting task", publication.sid());
}
