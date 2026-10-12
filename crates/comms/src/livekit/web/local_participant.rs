use crate::livekit::web::{
    request, DataPacket, LivekitCommand, LocalTrack, LocalTrackPublication, ParticipantIdentity,
    ParticipantSid, Reply, RoomError, RoomId, RoomResult, TrackPublishOptions,
};

/// The local participant of a room, as of the connect.
#[derive(Debug, Clone)]
pub struct LocalParticipant {
    pub(super) room: RoomId,
    pub(super) sid: ParticipantSid,
    pub(super) identity: ParticipantIdentity,
    pub(super) metadata: String,
}

impl LocalParticipant {
    pub async fn publish_data(&self, data: DataPacket) -> RoomResult<()> {
        let room = self.room;
        match request(|request| LivekitCommand::PublishData {
            request,
            room,
            packet: data,
        })
        .await?
        {
            Reply::PublishData(result) => result.map_err(RoomError),
            other => Err(RoomError(format!(
                "unexpected reply to publish_data: {other:?}"
            ))),
        }
    }

    pub async fn publish_track(
        &self,
        track: LocalTrack,
        options: TrackPublishOptions,
    ) -> RoomResult<LocalTrackPublication> {
        let room = self.room;
        let track = track.id();
        match request(|request| LivekitCommand::PublishTrack {
            request,
            room,
            track,
            options,
        })
        .await?
        {
            Reply::Publish(result) => result.map_err(RoomError),
            other => Err(RoomError(format!(
                "unexpected reply to publish_track: {other:?}"
            ))),
        }
    }

    pub async fn unpublish_track(
        &self,
        local_track: &LocalTrack,
    ) -> RoomResult<LocalTrackPublication> {
        let room = self.room;
        let track = local_track.id();
        match request(|request| LivekitCommand::UnpublishTrack {
            request,
            room,
            track,
        })
        .await?
        {
            Reply::Publish(result) => result.map_err(RoomError),
            other => Err(RoomError(format!(
                "unexpected reply to unpublish_track: {other:?}"
            ))),
        }
    }

    pub fn is_local(&self) -> bool {
        // Should always be true
        true
    }

    pub fn identity(&self) -> ParticipantIdentity {
        self.identity.clone()
    }

    pub fn sid(&self) -> ParticipantSid {
        self.sid.clone()
    }

    pub fn metadata(&self) -> String {
        self.metadata.clone()
    }
}
