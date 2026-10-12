use media::MediaId;

use crate::livekit::web::{send_command, LivekitCommand, ObjectId};

#[derive(Debug, Clone)]
pub struct RemoteVideoTrack {
    pub(super) id: ObjectId,
    pub(super) sid: String,
}

impl RemoteVideoTrack {
    /// Attaches the track's page video element to the media host as `media_id` (the id of a
    /// [`media::HtmlMedia::new_adopted`] handle).
    pub fn attach(&self, media_id: MediaId) {
        send_command(LivekitCommand::AttachVideo {
            track: self.id,
            media_id,
        });
    }

    pub fn sid(&self) -> String {
        self.sid.clone()
    }
}
