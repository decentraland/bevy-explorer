use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub enum LivekitUpdate {
    Availability(ConnectionAvailability),
    DisconnectReason(LivekitDisconnect),
    ConnectionQuality(LivekitParticipantConnectionQuality),
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
pub enum ConnectionAvailability {
    #[default]
    Available,
    /// If client is disconnected from room due to duplicate identity
    /// or from being kicked, client won't try connecting again
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct LivekitDisconnect {
    pub room: String,
    pub disconnect_reason: DisconnectReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub enum DisconnectReason {
    DuplicateIdentity,
    ParticipantRemoved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct LivekitParticipantConnectionQuality {
    pub participant: String,
    pub room: String,
    pub connection_quality: ConnectionQuality,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub enum ConnectionQuality {
    Excellent,
    Good,
    Poor,
    Lost,
}
