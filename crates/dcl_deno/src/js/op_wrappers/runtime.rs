use common::rpc::ReadFileResponse;
use dcl::js::runtime::SceneInfoResponse;
use dcl_component::proto_components::sdk::components::PbRealmInfo;
use deno_core::{error::AnyError, op2, OpDecl, OpState};
use std::{cell::RefCell, rc::Rc};

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![
        op_read_file(),
        op_scene_information(),
        op_realm_information(),
    ]
}

#[op2(async)]
#[serde]
async fn op_read_file(
    op_state: Rc<RefCell<OpState>>,
    #[string] filename: String,
) -> Result<ReadFileResponse, AnyError> {
    dcl::js::runtime::op_read_file(op_state, filename).await
}

#[op2(async)]
#[serde]
async fn op_scene_information(
    op_state: Rc<RefCell<OpState>>,
) -> Result<SceneInfoResponse, AnyError> {
    dcl::js::runtime::op_scene_information(op_state).await
}

#[op2(async)]
#[serde]
async fn op_realm_information(op_state: Rc<RefCell<OpState>>) -> Result<PbRealmInfo, AnyError> {
    dcl::js::runtime::op_realm_information(op_state).await
}
