use std::{cell::RefCell, rc::Rc};

use deno_core::{op2, OpDecl, OpState};

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![op_get_connected_players(), op_get_players_in_scene()]
}

#[op2(async)]
#[serde]
async fn op_get_connected_players(state: Rc<RefCell<OpState>>) -> Vec<String> {
    dcl::js::player::op_get_connected_players(state).await
}

#[op2(async)]
#[serde]
async fn op_get_players_in_scene(state: Rc<RefCell<OpState>>) -> Vec<String> {
    dcl::js::player::op_get_players_in_scene(state).await
}
