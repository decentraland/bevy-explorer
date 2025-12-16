use bevy::prelude::*;
use futures_lite::StreamExt;
use livekit::{
    prelude::RemoteTrackPublication,
    track::RemoteVideoTrack,
    webrtc::{
        native::yuv_helper,
        prelude::{I420Buffer, VideoBuffer},
    },
};
use tokio::sync::mpsc;

#[derive(Deref)]
pub struct LivekitVideoFrame {
    #[deref]
    buffer: I420Buffer,
    timestamp: i64,
}

impl LivekitVideoFrame {
    pub fn timestamp(&self) -> i64 {
        self.timestamp
    }

    pub fn width(&self) -> u32 {
        self.buffer.width()
    }

    pub fn height(&self) -> u32 {
        self.buffer.height()
    }

    pub fn rgba_data(&self) -> Vec<u8> {
        let width = self.buffer.width();
        let height = self.buffer.height();
        let stride = width * 4;

        let (stride_y, stride_u, stride_v) = self.buffer.strides();
        let (data_y, data_u, data_v) = self.buffer.data();

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
    channel: mpsc::Sender<LivekitVideoFrame>,
) {
    let mut stream =
        livekit::webrtc::video_stream::native::NativeVideoStream::new(video.rtc_track());

    while let Some(frame) = stream.next().await {
        let buffer = frame.buffer.to_i420();
        let Err(err) = channel
            .send(LivekitVideoFrame {
                buffer,
                timestamp: frame.timestamp_us,
            })
            .await
        else {
            continue;
        };

        error!("Livekit video channel errored: {err}.");
        break;
    }

    warn!("video track {:?} ended, exiting task", publication.sid());
}
