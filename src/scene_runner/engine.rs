// Engine module

use super::{EngineCommand, EngineCommandList, EngineResponseList};
use bevy::prelude::error;
use deno_core::{op, OpDecl, OpState};
use std::{cell::RefCell, rc::Rc};

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![op_engine_send_message::decl()]
}

// our op definitions
#[op(v8)]
async fn op_engine_send_message<'a>(
    op_state: Rc<RefCell<OpState>>,
    messages: Vec<String>,
) -> Vec<String> {
    let mut op_state = op_state.borrow_mut();

    // collect commands
    if !messages.is_empty() {
        let commands = op_state.borrow_mut::<EngineCommandList>();

        for message in messages {
            let message: Result<EngineCommand, _> = serde_json::from_str(message.as_str());
            match message {
                Ok(message) => commands.0.push(message),
                Err(e) => error!("failed to parse message: {}", e),
            }
        }
    }

    // return responses
    let responses = op_state.borrow::<EngineResponseList>();
    responses
        .0
        .iter()
        .map(|response| serde_json::to_string(response).unwrap())
        .collect()
}
