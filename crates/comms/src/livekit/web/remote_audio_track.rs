use crate::livekit::web::{send_command, LivekitCommand, ObjectId};

#[derive(Debug, Clone)]
pub struct RemoteAudioTrack {
    pub(super) id: ObjectId,
    pub(super) sid: String,
}

impl RemoteAudioTrack {
    pub fn set_volume(&self, volume: f32) {
        send_command(LivekitCommand::SetVolume {
            track: self.id,
            volume,
        });
    }

    pub fn sid(&self) -> String {
        self.sid.clone()
    }
}
