use std::{cell::RefCell, rc::Rc};

use dcl_component::proto_components::kernel::apis::VideoTracksActiveStreamsResponse;
use deno_core::{anyhow, op2, OpDecl, OpState};

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![op_get_active_video_streams()]
}

#[op2(async)]
#[serde]
async fn op_get_active_video_streams(
    state: Rc<RefCell<OpState>>,
) -> Result<VideoTracksActiveStreamsResponse, anyhow::Error> {
    dcl::js::comms_api::op_get_active_video_streams(state).await
}
