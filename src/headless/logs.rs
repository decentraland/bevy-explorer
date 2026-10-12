//! A scene's console output, drained each tick for whatever carries it out of the server: the
//! native binary prints `@scene-log` lines, the web entry posts them to its page.

use std::collections::HashMap;

use bevy::prelude::{Entity, Query};
use common::util::RingBufferReceiver;
use dcl::{SceneLogLevel, SceneLogMessage};
use scene_runner::renderer_context::RendererSceneContext;

pub type SceneLogReceivers = HashMap<Entity, RingBufferReceiver<SceneLogMessage>>;

/// Every scene's lines not yet emitted, in order, with its hash: a scene's backlog the first time
/// it is seen, then what it logged since.
pub fn drain_scene_logs(
    scenes: &Query<(Entity, &RendererSceneContext)>,
    receivers: &mut SceneLogReceivers,
    mut emit: impl FnMut(&str, &SceneLogMessage),
) {
    for (ent, ctx) in scenes.iter() {
        let rx = receivers.entry(ent).or_insert_with(|| {
            let (_missed, backlog, rx) = ctx.logs.read();
            for log in backlog {
                emit(&ctx.hash, &log);
            }
            rx
        });
        while let Ok(log) = rx.try_recv() {
            emit(&ctx.hash, &log);
        }
    }
    receivers.retain(|ent, _| scenes.contains(*ent));
}

pub fn scene_log_level(level: &SceneLogLevel) -> &'static str {
    match level {
        SceneLogLevel::Log => "log",
        SceneLogLevel::SceneError => "error",
        SceneLogLevel::SystemError => "system",
    }
}
