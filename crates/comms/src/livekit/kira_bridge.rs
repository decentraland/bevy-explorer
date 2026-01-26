use bevy::prelude::*;
use common::structs::AudioDecoderError;
use futures_lite::StreamExt;
use kira::sound::streaming::StreamingSoundData;
use livekit::{
    prelude::RemoteTrackPublication, track::RemoteAudioTrack, webrtc::prelude::AudioFrame,
};
use tokio::sync::mpsc;

struct LivekitKiraBridge {
    started: bool,
    sample_rate: u32,
    receiver: mpsc::Receiver<AudioFrame<'static>>,
}

impl kira::sound::streaming::Decoder for LivekitKiraBridge {
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
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    return Err(AudioDecoderError::StreamClosed)
                }
                Err(mpsc::error::TryRecvError::Empty) => return Ok(frames),
            }
        }
    }

    fn seek(&mut self, seek: usize) -> Result<usize, Self::Error> {
        if !self.started && seek == 0 {
            return Ok(0);
        }
        Err(AudioDecoderError::Other(format!(
            "Can't seek (requested {seek})"
        )))
    }
}

pub async fn kira_thread(
    audio: RemoteAudioTrack,
    publication: RemoteTrackPublication,
    channel: tokio::sync::oneshot::Sender<StreamingSoundData<AudioDecoderError>>,
) {
    let mut stream =
        livekit::webrtc::audio_stream::native::NativeAudioStream::new(audio.rtc_track(), 48_000, 1);

    // get first frame to set sample rate
    let Some(frame) = stream.next().await else {
        warn!("dropped audio track without samples");
        return;
    };

    let (frame_sender, frame_receiver) = mpsc::channel(1000);

    let bridge = LivekitKiraBridge {
        started: false,
        sample_rate: frame.sample_rate,
        receiver: frame_receiver,
    };

    debug!("recced with {} / {}", frame.sample_rate, frame.num_channels);

    let sound_data = kira::sound::streaming::StreamingSoundData::from_decoder(bridge);

    let res = channel.send(sound_data);

    if res.is_err() {
        warn!("failed to send subscribed audio data");
        publication.set_subscribed(false);
        return;
    }

    while let Some(frame) = stream.next().await {
        match frame_sender.try_send(frame) {
            Ok(()) => (),
            Err(mpsc::error::TrySendError::Full(_)) => {
                warn!("livekit audio receiver buffer full, dropping frame");
                return;
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                warn!("livekit audio receiver dropped, exiting task");
                return;
            }
        }
    }

    warn!("track ended, exiting task");
}
