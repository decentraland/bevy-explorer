use crate::livekit::web::{
    send_command, LivekitCommand, ObjectId, RemoteAudioTrack, RemoteVideoTrack,
};

#[derive(Clone, Debug)]
pub enum RemoteTrack {
    Audio(RemoteAudioTrack),
    Video(RemoteVideoTrack),
}

impl RemoteTrack {
    pub fn pan_and_volume(&self, pan: f32, volume: f32) {
        send_command(LivekitCommand::PanAndVolume {
            track: self.id(),
            pan,
            volume,
        });
    }

    pub(super) fn id(&self) -> ObjectId {
        match self {
            RemoteTrack::Audio(audio) => audio.id,
            RemoteTrack::Video(video) => video.id,
        }
    }
}
