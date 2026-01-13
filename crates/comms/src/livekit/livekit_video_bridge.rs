use std::{borrow::Borrow, fmt::Display};

use bevy::prelude::*;
use bevy_kira_audio::prelude::Frame;
use futures_lite::StreamExt;
use livekit::{
    prelude::RemoteTrackPublication,
    track::{RemoteAudioTrack, RemoteVideoTrack},
    webrtc::{
        audio_stream::native::NativeAudioStream,
        native::yuv_helper,
        prelude::{AudioFrame, I420Buffer, VideoBuffer},
        video_stream::native::NativeVideoStream,
    },
};
use tokio::sync::mpsc;

pub trait AudioFrameExt {
    fn to_frame(&self) -> Result<Vec<Frame>, TryIntoFrame>;
}

impl AudioFrameExt for AudioFrame<'_> {
    fn to_frame(&self) -> Result<Vec<Frame>, TryIntoFrame> {
        match self.num_channels {
            0 => Err(TryIntoFrame::NoChannels),
            1 => Ok(self
                .data
                .iter()
                .map(i16_to_f32_sample)
                .map(Frame::from_mono)
                .collect()),
            2 => {
                let (left, right) = self.data.split_at(self.data.len() / 2);
                let left_iter = left.iter().map(i16_to_f32_sample);
                let right_iter = right.iter().map(i16_to_f32_sample);
                Ok(left_iter
                    .zip(right_iter)
                    .map(|(left, right)| Frame::new(left, right))
                    .collect())
            }
            _ => Err(TryIntoFrame::NotMonoOrStereo),
        }
    }
}

fn i16_to_f32_sample(sample: impl Borrow<i16>) -> f32 {
    *sample.borrow() as f32 / i16::MAX as f32
}

#[derive(Debug)]
pub enum TryIntoFrame {
    NoChannels,
    NotMonoOrStereo,
}

impl Display for TryIntoFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoChannels => write!(f, "Audio frame had no channels."),
            Self::NotMonoOrStereo => write!(f, "Audio frame had more than 2 channels."),
        }
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

pub async fn livekit_audio_thread(
    audio: RemoteAudioTrack,
    channel: mpsc::Sender<AudioFrame<'static>>,
) {
    let mut stream = NativeAudioStream::new(audio.rtc_track(), 48_000, 1);

    while let Some(frame) = stream.next().await {
        debug!(
            "Frame for {} received. {} / {}",
            audio.sid(),
            frame.data.len(),
            frame.num_channels
        );
        match channel.send(frame).await {
            Ok(()) => (),
            Err(mpsc::error::SendError(_)) => {
                warn!("Failed to send audio frame through channel.");
                return;
            }
        }
    }

    warn!("track ended, exiting task");
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
