use std::{cell::RefCell, rc::Rc};

use bevy::log::debug;
use common::rpc::{RpcCall, RpcResultSender};
use dcl_component::proto_components::kernel::apis::VideoTracksActiveStreamsResponse;

use crate::{js::State, RpcCalls};

pub async fn op_get_active_video_streams(
    state: Rc<RefCell<impl State>>,
) -> Result<Option<VideoTracksActiveStreamsResponse>, anyhow::Error> {
    debug!("op_get_active_video_streams");
    let (sx, rx) = RpcResultSender::channel();

    {
        let mut state = state.borrow_mut();

        state
            .borrow_mut::<RpcCalls>()
            .push(RpcCall::ActiveVideoStreams { response: sx })?;
    }

    Ok(rx.await.unwrap_or_default())
}
