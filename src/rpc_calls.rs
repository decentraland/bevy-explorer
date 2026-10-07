use bevy::prelude::*;
use common::rpc::{RpcCall, RpcCallEvent};
use comms::{
    livekit::{
        participant::{HostingParticipants, LivekitParticipant},
        track::{
            Camera as LivekitSourceCamera, LivekitTrack, Publishing,
            ScreenshareVideo as LivekitSourceScreenshare, Video,
        },
    },
    SceneRoom, SceneRoomMap, Transport,
};
use dcl_component::proto_components::kernel::apis::{
    VideoTrackSourceType, VideoTracksActiveStreamsData, VideoTracksActiveStreamsResponse,
};
use scene_runner::{renderer_context::RendererSceneContext, ContainerEntity};

pub struct RpcCallsPlugin;

impl Plugin for RpcCallsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, respond_to_rpc_calls);
    }
}

#[expect(clippy::type_complexity, reason = "Queries are complex")]
fn respond_to_rpc_calls(
    mut rpc_calls: EventReader<RpcCallEvent>,
    scene_rooms: Res<SceneRoomMap>,
    scenes: Query<&RendererSceneContext>,
    transports: Query<&HostingParticipants, With<Transport>>,
    participants: Query<(&LivekitParticipant, &Publishing)>,
    tracks: Query<
        (
            &LivekitTrack,
            AnyOf<(&LivekitSourceCamera, &LivekitSourceScreenshare)>,
        ),
        With<Video>,
    >,
) {
    for RpcCallEvent { origin, call } in rpc_calls.read() {
        #[expect(clippy::single_match, reason = "May be expanded in the future")]
        match call {
            RpcCall::ActiveVideoStreams { response } => {
                let mut streams = vec![];
                let Some(scene) = origin.scene() else {
                    error!("ActiveVideoStreams RpcCall with no origin.");
                    response.send(VideoTracksActiveStreamsResponse { streams });
                    continue;
                };
                let Ok(renderer_context) = scenes.get(scene) else {
                    error!("RpcCall with invalid origin {}", scene);
                    response.send(VideoTracksActiveStreamsResponse { streams });
                    continue;
                };
                let Some(transport) = scene_rooms.get(&*renderer_context.hash) else {
                    debug!("No mapping between RpcCall origin and a transport.");
                    response.send(VideoTracksActiveStreamsResponse { streams });
                    continue;
                };
                let Ok(hosting_participants) = transports.get(*transport) else {
                    error!("Mapping between RpcCall origin and a transport is invalid.");
                    response.send(VideoTracksActiveStreamsResponse { streams });
                    continue;
                };

                for (participant, publishing) in
                    participants.iter_many(hosting_participants.collection())
                {
                    for (track, source) in tracks.iter_many(publishing.collection()) {
                        streams.push(VideoTracksActiveStreamsData {
                            identity: participant.identity().0,
                            track_sid: track.sid().to_string(),
                            source_type: match source {
                                (Some(_), _) => VideoTrackSourceType::VtstCamera as i32,
                                (_, Some(_)) => VideoTrackSourceType::VtstScreenShare as i32,
                                _ => VideoTrackSourceType::VtstUnknown as i32,
                            },
                        });
                    }
                }
                response.send(VideoTracksActiveStreamsResponse { streams });
            }
            _ => (),
        }
    }
}
