use std::{cell::RefCell, rc::Rc};

use dcl::js::user_identity::UserData;
use deno_core::{error::AnyError, op2, OpDecl, OpState};

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![op_get_user_data(), op_get_player_data()]
}
pub struct UserEthAddress(pub String);

#[op2(async)]
#[serde]
async fn op_get_user_data(state: Rc<RefCell<OpState>>) -> Result<UserData, AnyError> {
    dcl::js::user_identity::op_get_user_data(state).await
}

#[op2(async)]
#[serde]
async fn op_get_player_data(
    state: Rc<RefCell<OpState>>,
    #[string] id: String,
) -> Result<UserData, AnyError> {
    dcl::js::user_identity::op_get_player_data(state, id).await
}
