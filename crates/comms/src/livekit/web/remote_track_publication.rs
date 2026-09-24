use crate::livekit::web::{
    send_command, LivekitCommand, ObjectId, RemoteTrack, TrackKind, TrackSource,
};

/// A remote track publication, as of the event that carried it (`track` is the subscribed
/// track, if any, at that time).
#[derive(Debug, Clone)]
pub struct RemoteTrackPublication {
    pub(super) id: ObjectId,
    pub(super) sid: String,
    pub(super) kind: TrackKind,
    pub(super) source: TrackSource,
    pub(super) track: Option<RemoteTrack>,
}

impl RemoteTrackPublication {
    pub fn sid(&self) -> String {
        self.sid.clone()
    }

    pub fn kind(&self) -> TrackKind {
        self.kind
    }

    pub fn source(&self) -> TrackSource {
        self.source
    }

    pub fn set_subscribed(&self, subscribed: bool) {
        send_command(LivekitCommand::SetSubscribed {
            publication: self.id,
            subscribed,
        });
    }

    pub fn track(&self) -> Option<RemoteTrack> {
        self.track.clone()
    }
}
