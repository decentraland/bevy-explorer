use crate::livekit::web::{
    LocalParticipant, ParticipantIdentity, ParticipantSid, RemoteParticipant,
};

#[derive(Debug, Clone)]
pub enum Participant {
    Local(LocalParticipant),
    Remote(RemoteParticipant),
}

impl Participant {
    pub fn identity(&self) -> ParticipantIdentity {
        match self {
            Self::Local(l) => l.identity(),
            Self::Remote(r) => r.identity(),
        }
    }

    pub fn sid(&self) -> ParticipantSid {
        match self {
            Self::Local(l) => l.sid(),
            Self::Remote(r) => r.sid(),
        }
    }

    pub fn metadata(&self) -> String {
        match self {
            Self::Local(l) => l.metadata(),
            Self::Remote(r) => r.metadata(),
        }
    }
}
