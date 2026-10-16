//! "Explorer Scene Load Times": one event per scene load, when the scene is first running (the
//! lifecycle's "no longer loading" bar: past tick 6, or frozen there) or when it failed.

use analytics::{
    data_definition::{SegmentEvent, SegmentEventExplorerSceneLoadTimes},
    segment_system::SegmentMetricsEvents,
};
use bevy::{
    platform::collections::{HashMap, HashSet},
    prelude::*,
};

use crate::{
    initialize_scene::{SceneHash, SceneLoading, SuperUserScene},
    renderer_context::{RendererSceneContext, FROZEN_BLOCK},
};

#[derive(Default)]
pub struct LoadTracker {
    // load start, or None once reported
    started: HashMap<Entity, Option<f64>>,
}

#[allow(clippy::type_complexity)]
pub fn report_scene_load_times(
    scenes: Query<
        (
            Entity,
            Option<&SceneHash>,
            Option<&SceneLoading>,
            Option<&RendererSceneContext>,
        ),
        (
            Or<(With<SceneLoading>, With<RendererSceneContext>)>,
            Without<SuperUserScene>,
        ),
    >,
    time: Res<Time>,
    metrics: Option<ResMut<SegmentMetricsEvents>>,
    mut tracker: Local<LoadTracker>,
) {
    let Some(mut metrics) = metrics else {
        return;
    };
    let now = time.elapsed_secs_f64();
    let mut seen = HashSet::new();

    for (entity, hash, loading, context) in scenes.iter() {
        seen.insert(entity);
        let Some(started) = *tracker.started.entry(entity).or_insert(Some(now)) else {
            continue;
        };

        let success = match (loading, context) {
            (Some(SceneLoading::Failed), _) => false,
            (_, Some(ctx)) if ctx.broken() => false,
            (_, Some(ctx)) if ctx.tick_number > 6 || ctx.blocked.contains(FROZEN_BLOCK) => true,
            _ => continue,
        };
        tracker.started.insert(entity, None);
        let Some(scene_hash) = hash
            .map(|h| h.0.clone())
            .or_else(|| context.map(|ctx| ctx.hash.to_string()))
        else {
            continue;
        };

        metrics.add_event(SegmentEvent::ExplorerSceneLoadTimes(
            SegmentEventExplorerSceneLoadTimes {
                scene_hash,
                elapsed: (now - started) as f32,
                success,
            },
        ));
    }

    // a scene despawned before it was ready (left behind, realm change) is never reported
    tracker.started.retain(|entity, _| seen.contains(entity));
}
