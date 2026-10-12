use crate::livekit::web::{LocalAudioTrack, LocalVideoTrack, ObjectId};

pub enum LocalTrack {
    Audio(LocalAudioTrack),
    Video(LocalVideoTrack),
}

impl LocalTrack {
    pub(super) fn id(&self) -> ObjectId {
        match self {
            LocalTrack::Audio(audio) => audio.id,
            LocalTrack::Video(video) => video.id,
        }
    }
}
