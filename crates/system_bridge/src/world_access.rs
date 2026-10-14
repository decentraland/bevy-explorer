//! Relays the engine's world password prompt (`ipfs::world_access`) to the HUD as a stream and
//! the HUD's answer back.

use std::fmt;

use bevy::prelude::*;
use common::rpc::RpcStreamSender;
use common::structs::{WorldPasswordPrompt, WorldPasswordPrompter};
use serde::{Deserialize, Serialize};
use system_api_types::{WorldAccessEvent, WorldPasswordRequest};
use tokio::sync::mpsc;

use crate::SystemApi;

/// A password on its way to the engine. Its `Debug` hides it: system api events are logged.
#[derive(Clone, Serialize, Deserialize)]
pub struct WorldPassword(pub String);

impl fmt::Debug for WorldPassword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WorldPassword(<redacted>)")
    }
}

#[derive(Resource)]
pub struct WorldPasswordPrompts {
    incoming: mpsc::UnboundedReceiver<WorldPasswordPrompt>,
    /// The prompt awaiting the HUD's answer. Only the latest realm change gets one: a new prompt
    /// gives up the one before it.
    pending: Option<WorldPasswordPrompt>,
    streams: Vec<RpcStreamSender<WorldAccessEvent>>,
}

pub fn install_prompter(app: &mut App) {
    let (sender, incoming) = mpsc::unbounded_channel();
    app.insert_resource(WorldPasswordPrompter(sender));
    app.insert_resource(WorldPasswordPrompts {
        incoming,
        pending: None,
        streams: Vec::new(),
    });
}

fn request(prompt: &WorldPasswordPrompt) -> WorldAccessEvent {
    WorldAccessEvent::PasswordRequest(WorldPasswordRequest {
        world: prompt.world.clone(),
        error: prompt.error.clone(),
    })
}

pub fn handle_world_access(
    mut prompts: ResMut<WorldPasswordPrompts>,
    mut events: EventReader<SystemApi>,
) {
    let prompts = &mut *prompts;
    for ev in events.read() {
        match ev {
            SystemApi::GetWorldAccessStream(stream) => {
                if let Some(prompt) = &prompts.pending {
                    let _ = stream.send(request(prompt));
                }
                prompts.streams.push(stream.clone());
            }
            SystemApi::SetWorldPassword(password) => match prompts.pending.take() {
                Some(prompt) => {
                    let _ = prompt
                        .answer
                        .send(password.as_ref().map(|password| password.0.clone()));
                }
                None => warn!("no world password prompt to answer"),
            },
            _ => (),
        }
    }

    prompts.streams.retain(|stream| !stream.is_closed());

    // the prompt's realm change is over (superseded, or the player left): nothing to answer
    if prompts
        .pending
        .as_ref()
        .is_some_and(|prompt| prompt.answer.is_closed())
    {
        prompts.pending = None;
        for stream in &prompts.streams {
            let _ = stream.send(WorldAccessEvent::Cancelled);
        }
    }

    while let Ok(prompt) = prompts.incoming.try_recv() {
        if prompts.streams.is_empty() {
            // nothing to show it: the world can't be entered rather than hang
            let _ = prompt.answer.send(None);
            continue;
        }
        if let Some(superseded) = prompts.pending.take() {
            let _ = superseded.answer.send(None);
        }
        for stream in &prompts.streams {
            let _ = stream.send(request(&prompt));
        }
        prompts.pending = Some(prompt);
    }
}
