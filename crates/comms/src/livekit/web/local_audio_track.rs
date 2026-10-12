use crate::livekit::web::{
    request, AudioCaptureOptions, LivekitCommand, ObjectId, Reply, RoomError, RoomResult, TrackSid,
};

#[derive(Debug, Clone)]
pub struct LocalAudioTrack {
    pub(super) id: ObjectId,
    pub(super) sid: TrackSid,
}

impl LocalAudioTrack {
    pub async fn new(options: AudioCaptureOptions) -> RoomResult<Self> {
        match request(|request| LivekitCommand::CreateLocalAudioTrack { request, options }).await? {
            Reply::LocalAudioTrack(result) => result.map_err(RoomError),
            other => Err(RoomError(format!(
                "unexpected reply to local audio track: {other:?}"
            ))),
        }
    }

    /// A track the page could not create; publishing it fails.
    pub fn unavailable() -> Self {
        Self {
            id: 0,
            sid: TrackSid(String::new()),
        }
    }

    pub fn sid(&self) -> TrackSid {
        self.sid.clone()
    }
}
