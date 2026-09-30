use crate::livekit::web::{ObjectId, ParticipantIdentity, ParticipantSid};

/// A remote participant, as of the event that carried it.
#[derive(Debug, Clone)]
pub struct RemoteParticipant {
    pub(super) id: ObjectId,
    pub(super) sid: ParticipantSid,
    pub(super) identity: ParticipantIdentity,
    pub(super) metadata: String,
}

impl RemoteParticipant {
    pub fn is_local(&self) -> bool {
        // Should always be false
        false
    }

    pub fn identity(&self) -> ParticipantIdentity {
        self.identity.clone()
    }

    // pub fn name(&self) -> String {
    //     "".to_owned()
    // }

    pub fn metadata(&self) -> String {
        self.metadata.clone()
    }

    pub fn sid(&self) -> ParticipantSid {
        self.sid.clone()
    }
}
