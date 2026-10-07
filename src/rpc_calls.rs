use bevy::prelude::*;
use common::rpc::{RpcCall, RpcCallEvent};
use dcl_component::proto_components::kernel::apis::VideoTracksActiveStreamsResponse;

pub struct RpcCallsPlugin;

impl Plugin for RpcCallsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, respond_to_rpc_calls);
    }
}

fn respond_to_rpc_calls(mut commands: Commands, mut rpc_calls: EventReader<RpcCallEvent>) {
    for RpcCallEvent { origin, call } in rpc_calls.read() {
        #[expect(clippy::single_match, reason = "May be expanded in the future")]
        match call {
            RpcCall::ActiveVideoStreams { response } => {
                commands.entity(origin.scene().unwrap()).log_components();
                response.send(VideoTracksActiveStreamsResponse { streams: vec![] });
            }
            _ => (),
        }
    }
}
