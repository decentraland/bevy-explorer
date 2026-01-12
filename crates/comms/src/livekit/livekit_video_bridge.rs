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
    let mut stream =
        livekit::webrtc::video_stream::native::NativeVideoStream::new(video.rtc_track());

    while let Some(frame) = stream.next().await {
        let buffer = frame.buffer.to_i420();
        if let Err(err) = sender.send(buffer).await {
            error!("Livekit video thread failed to send frame buffer due to '{err}'.");
            break;
        }
    }

    warn!("video track {:?} ended, exiting task", publication.sid());
}
