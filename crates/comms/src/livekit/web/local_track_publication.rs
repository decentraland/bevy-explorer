use crate::livekit::web::{TrackKind, TrackSource};

#[derive(Debug, Clone)]
pub struct LocalTrackPublication {
    pub(super) sid: String,
    pub(super) kind: TrackKind,
    pub(super) source: TrackSource,
}

impl LocalTrackPublication {
    pub fn sid(&self) -> String {
        self.sid.clone()
    }

    pub fn kind(&self) -> TrackKind {
        self.kind
    }

    pub fn source(&self) -> TrackSource {
        self.source
    }
}
