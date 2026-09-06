//! Reports avatar emote playback to scenes as `AvatarEmoteCommand` entries: a start with the
//! emote's resolved loop flag, then how it ended (`ES_FINISHED` / `ES_INTERRUPTED`), the way the
//! reference client writes them.
//!
//! Render-free, so the client and the headless server run the same code over the same inputs.
//! Foreign players' transitions come off the wire (raised by `comms`, in wire order); the local
//! player's and scene avatars' from playback (`animate`). Every transition is reported, in order:
//! a start whose loop flag is still loading holds the entries behind it for that avatar rather
//! than being dropped, so what a scene hears never depends on which cache happened to be warm.
//!
//! Entries go to the scenes the avatar is in (a scene avatar's own scene), each scene's crdt
//! alone, never the history. While an emote plays that set is kept in step: a scene the avatar
//! moves out of (or that unloads) hears the emote end there, one it moves into (or that loads
//! around it) hears it start, the way the reference client adds and removes the player's
//! command in each scene.

use std::collections::VecDeque;

use bevy::{
    platform::collections::{HashMap, HashSet},
    prelude::*,
};
use collectibles::{CollectibleError, CollectibleManager, Emote, EmoteUrn};
use common::{
    dynamics::PLAYER_COLLIDER_RADIUS,
    structs::{
        EmoteCommand, EmoteLifecycle, EmoteLifecycleEvent, EmoteLifecycleSource, PrimaryUser,
    },
};
use comms::global_crdt::{process_transport_updates, CrdtContexts, ForeignPlayer};
use dcl::interface::CrdtType;
use dcl_component::{
    proto_components::sdk::components::{EmoteState, PbAvatarEmoteCommand},
    SceneComponentId, SceneEntityId,
};
use scene_runner::{renderer_context::RendererSceneContext, ContainerEntity, ContainingScene};

/// Transitions one avatar may hold behind a start whose loop flag is still loading. Past this the
/// start goes out with the flag it was triggered with rather than delaying the rest any longer.
const MAX_PENDING: usize = 16;
/// Longest a start waits on its loop flag. The collectible manager settles a bad urn as `Missing`
/// or `Failed`, but an unreachable content server re-requests forever.
const MAX_RESOLVE_SECS: f64 = 30.0;

pub struct EmoteReportPlugin;

impl Plugin for EmoteReportPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<EmoteLifecycleEvent>();
        app.add_systems(
            Update,
            (queue_emote_reports, flush_emote_reports)
                .chain()
                .after(process_transport_updates),
        );
    }
}

/// Per-avatar report state. Despawns with the avatar.
#[derive(Component, Default)]
pub struct EmoteReportQueue {
    pending: VecDeque<EmoteLifecycle>,
    /// The start the scenes have been told about, `(urn, loop)`, echoed by its stop entry.
    reported: Option<(String, bool)>,
    /// The scenes told `reported`.
    told: HashSet<Entity>,
    /// `timestamp` of the last entry written. The sdk's grow-only set orders and trims by it, so
    /// it only has to be monotonic per avatar.
    sequence: u32,
    /// Last `EmoteCommand` seen, so a rewrite of the same command isn't a new start.
    last_command: Option<EmoteCommand>,
    /// When the head start began waiting on its loop flag.
    waiting_since: Option<f64>,
}

#[allow(clippy::type_complexity)]
fn queue_emote_reports(
    mut commands: Commands,
    mut queues: Query<&mut EmoteReportQueue>,
    // Anything writing `EmoteCommand` on a local or scene avatar is a start. Foreign avatars are
    // driven by the wire alone (see `process_transport_updates`).
    triggered: Query<(Entity, &EmoteCommand), (Changed<EmoteCommand>, Without<ForeignPlayer>)>,
    foreign: Query<(), With<ForeignPlayer>>,
    mut events: EventReader<EmoteLifecycleEvent>,
) {
    // this frame's entries per avatar, and the command that produced a start
    let mut incoming: HashMap<Entity, (VecDeque<EmoteLifecycle>, Option<EmoteCommand>)> =
        HashMap::default();

    for (avatar, command) in &triggered {
        let last_command = queues
            .get(avatar)
            .ok()
            .and_then(|queue| queue.last_command.as_ref());
        if last_command == Some(command) {
            continue;
        }
        let (pending, last_command) = incoming.entry(avatar).or_default();
        if !command.urn.is_empty() {
            pending.push_back(EmoteLifecycle::Started {
                urn: command.urn.clone(),
                r#loop: command.r#loop,
            });
        }
        *last_command = Some(command.clone());
    }

    // the wire's word for a foreign avatar, playback's for the rest; the other is what this
    // binary happens to observe, and the two would disagree on timing
    for EmoteLifecycleEvent {
        avatar,
        event,
        source,
    } in events.read()
    {
        let authoritative = if foreign.contains(*avatar) {
            EmoteLifecycleSource::Wire
        } else {
            EmoteLifecycleSource::Playback
        };
        if *source != authoritative {
            continue;
        }
        incoming
            .entry(*avatar)
            .or_default()
            .0
            .push_back(event.clone());
    }

    for (avatar, (pending, last_command)) in incoming {
        if let Ok(mut queue) = queues.get_mut(avatar) {
            queue.pending.extend(pending);
            if last_command.is_some() {
                queue.last_command = last_command;
            }
        } else if let Ok(mut avatar) = commands.get_entity(avatar) {
            avatar.insert(EmoteReportQueue {
                pending,
                last_command,
                ..default()
            });
        }
    }
}

#[allow(clippy::type_complexity)]
fn flush_emote_reports(
    mut queues: Query<(
        Entity,
        &mut EmoteReportQueue,
        Option<&ForeignPlayer>,
        Option<&PrimaryUser>,
        Option<&ContainerEntity>,
    )>,
    mut emotes: CollectibleManager<Emote>,
    mut scenes: Query<&mut RendererSceneContext>,
    containing_scene: ContainingScene,
    crdt_contexts: Res<CrdtContexts>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs_f64();

    for (avatar, mut queue, foreign, primary, container) in queues.iter_mut() {
        if queue.pending.is_empty() && queue.reported.is_none() {
            continue;
        }

        // Which scenes hear this avatar, and as which entity. A player: the scenes around it that
        // share its crdt context (all of them on a client; its room's on a multi-tenant server).
        let (targets, id) = match (foreign, primary, container) {
            (Some(foreign), ..) => {
                let targets = containing_scene
                    .get_area(avatar, PLAYER_COLLIDER_RADIUS)
                    .into_iter()
                    .filter(|scene| {
                        scenes.get(*scene).is_ok_and(|scene| {
                            crdt_contexts.try_for_scene_hash(&scene.hash) == Some(foreign.context)
                        })
                    })
                    .collect::<Vec<_>>();
                (targets, foreign.scene_id)
            }
            (None, Some(_), _) => (
                containing_scene
                    .get_area(avatar, PLAYER_COLLIDER_RADIUS)
                    .into_iter()
                    .collect(),
                SceneEntityId::PLAYER,
            ),
            (None, None, Some(container)) => (vec![container.root], container.container_id),
            // e.g. the backpack preview avatar: no scene to tell
            _ => (Vec::new(), SceneEntityId::ROOT),
        };

        queue.migrate(&targets, |scene, urn, loops, state, timestamp| {
            if let Ok(mut scene) = scenes.get_mut(scene) {
                write_entry(&mut scene, id, urn, loops, state, timestamp);
            }
        });
        queue.advance(
            now,
            |urn, hint, force| resolve_loop(&mut emotes, urn, hint, force),
            |urn, loops, state, timestamp| {
                for scene in &targets {
                    if let Ok(mut scene) = scenes.get_mut(*scene) {
                        write_entry(&mut scene, id, urn, loops, state, timestamp);
                    }
                }
            },
        );
        queue.heard(&targets);
    }
}

fn write_entry(
    scene: &mut RendererSceneContext,
    id: SceneEntityId,
    urn: &str,
    loops: bool,
    state: EmoteState,
    timestamp: u32,
) {
    let entry = PbAvatarEmoteCommand {
        emote_urn: urn.to_owned(),
        r#loop: loops,
        timestamp,
        mask: None,
        state: Some(state as i32),
    };
    debug!("emote report to {}: {entry:?}", scene.hash);
    scene.update_crdt(
        SceneComponentId::AVATAR_EMOTE_COMMAND,
        CrdtType::GO_ANY,
        id,
        &entry,
    );
}

impl EmoteReportQueue {
    /// Keep the reported start in step with the scenes that now hear the avatar: one it has left
    /// hears the emote end there, one it has entered hears it start. `emit` writes an entry to
    /// one scene: `(scene, urn, loop, state, timestamp)`.
    fn migrate(
        &mut self,
        targets: &[Entity],
        mut emit: impl FnMut(Entity, &str, bool, EmoteState, u32),
    ) {
        let Some((urn, loops)) = self.reported.clone() else {
            return;
        };
        let left = self
            .told
            .iter()
            .copied()
            .filter(|scene| !targets.contains(scene))
            .collect::<Vec<_>>();
        for scene in left {
            self.told.remove(&scene);
            self.sequence += 1;
            emit(scene, &urn, loops, EmoteState::EsInterrupted, self.sequence);
        }
        let entered = targets
            .iter()
            .copied()
            .filter(|scene| !self.told.contains(scene))
            .collect::<Vec<_>>();
        for scene in entered {
            self.told.insert(scene);
            self.sequence += 1;
            emit(scene, &urn, loops, EmoteState::EsStarted, self.sequence);
        }
    }

    /// After [`Self::advance`] wrote to `targets`: which scenes know the reported start.
    fn heard(&mut self, targets: &[Entity]) {
        self.told.clear();
        if self.reported.is_some() {
            self.told.extend(targets.iter().copied());
        }
    }

    /// Drain the queue for as long as its head can be written. `resolve` gives a start's loop
    /// flag (`None` while it's loading; the last argument asks it to answer regardless);
    /// `emit` writes an entry: `(urn, loop, state, timestamp)`.
    fn advance(
        &mut self,
        now: f64,
        mut resolve: impl FnMut(&str, bool, bool) -> Option<bool>,
        mut emit: impl FnMut(&str, bool, EmoteState, u32),
    ) {
        while let Some(head) = self.pending.front() {
            let (urn, loops, state) = match head {
                EmoteLifecycle::Started { urn, r#loop } => {
                    let waiting_since = *self.waiting_since.get_or_insert(now);
                    let force =
                        self.pending.len() > MAX_PENDING || now - waiting_since > MAX_RESOLVE_SECS;
                    let Some(loops) = resolve(urn, *r#loop, force) else {
                        break;
                    };
                    (urn.clone(), loops, EmoteState::EsStarted)
                }
                EmoteLifecycle::Finished | EmoteLifecycle::Interrupted => {
                    let Some((urn, loops)) = self.reported.take() else {
                        // a stop for a start the scene never heard of
                        self.pending.pop_front();
                        continue;
                    };
                    let state = if matches!(head, EmoteLifecycle::Finished) {
                        EmoteState::EsFinished
                    } else {
                        EmoteState::EsInterrupted
                    };
                    (urn, loops, state)
                }
            };
            self.pending.pop_front();
            self.waiting_since = None;

            if state == EmoteState::EsStarted {
                // the previous play ends where the new one starts; stop first, like the
                // reference client
                if let Some((urn, loops)) = self.reported.take() {
                    self.sequence += 1;
                    emit(&urn, loops, EmoteState::EsInterrupted, self.sequence);
                }
                self.reported = Some((urn.clone(), loops));
            }
            self.sequence += 1;
            emit(&urn, loops, state, self.sequence);
        }
    }
}

/// The emote's loop flag: what it was triggered with, or what its metadata says. `None` while the
/// metadata is loading, unless `force`d.
fn resolve_loop(
    emotes: &mut CollectibleManager<Emote>,
    urn: &str,
    hint: bool,
    force: bool,
) -> Option<bool> {
    let Ok(parsed) = EmoteUrn::new(urn) else {
        return Some(hint);
    };
    // a scene emote carries it in the urn's trailing `-{loop}` token
    if let Some(scene_emote) = parsed.scene_emote() {
        return Some(hint || scene_emote.ends_with("-true"));
    }
    match emotes.get_data(&parsed) {
        Ok(data) => Some(hint || data.extra_data.loops),
        Err(CollectibleError::Loading) if !force => None,
        Err(e) => {
            debug!("reporting {urn} with the triggered loop flag: {e:?}");
            Some(hint)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SLOW: &str = "urn:decentraland:off-chain:base-emotes:slow";
    const CACHED: &str = "urn:decentraland:off-chain:base-emotes:cached";

    fn start(urn: &str) -> EmoteLifecycle {
        EmoteLifecycle::Started {
            urn: urn.to_owned(),
            r#loop: false,
        }
    }

    /// `SLOW` loops but resolves only once `slow_ready`; everything else is a one-shot, ready.
    fn advance(
        queue: &mut EmoteReportQueue,
        now: f64,
        slow_ready: bool,
    ) -> Vec<(String, bool, EmoteState, u32)> {
        let mut out = Vec::new();
        queue.advance(
            now,
            |urn, hint, force| match (urn == SLOW, slow_ready, force) {
                (true, false, false) => None,
                (true, true, _) => Some(true),
                _ => Some(hint),
            },
            |urn, loops, state, ts| out.push((urn.to_owned(), loops, state, ts)),
        );
        out
    }

    #[test]
    fn slow_start_holds_later_entries_in_order() {
        let mut queue = EmoteReportQueue::default();
        queue
            .pending
            .extend([start(SLOW), EmoteLifecycle::Interrupted, start(CACHED)]);
        // emote 2 is cached, but emote 1 hasn't resolved: nothing goes out yet
        assert!(advance(&mut queue, 0.0, false).is_empty());
        assert_eq!(
            advance(&mut queue, 1.0, true),
            vec![
                (SLOW.to_owned(), true, EmoteState::EsStarted, 1),
                (SLOW.to_owned(), true, EmoteState::EsInterrupted, 2),
                (CACHED.to_owned(), false, EmoteState::EsStarted, 3),
            ]
        );
        assert_eq!(queue.reported, Some((CACHED.to_owned(), false)));
    }

    #[test]
    fn supersede_stops_the_reported_start_first_and_orphan_stops_are_dropped() {
        let mut queue = EmoteReportQueue::default();
        queue
            .pending
            .extend([EmoteLifecycle::Finished, start(CACHED), start("other")]);
        assert_eq!(
            advance(&mut queue, 0.0, true),
            vec![
                (CACHED.to_owned(), false, EmoteState::EsStarted, 1),
                (CACHED.to_owned(), false, EmoteState::EsInterrupted, 2),
                ("other".to_owned(), false, EmoteState::EsStarted, 3),
            ]
        );
        queue.pending.push_back(EmoteLifecycle::Finished);
        queue.pending.push_back(EmoteLifecycle::Finished);
        assert_eq!(
            advance(&mut queue, 0.0, true),
            vec![("other".to_owned(), false, EmoteState::EsFinished, 4)]
        );
        assert!(queue.pending.is_empty());
    }

    #[test]
    fn a_full_queue_or_a_long_wait_forces_the_head_start() {
        let mut queue = EmoteReportQueue::default();
        queue.pending.push_back(start(SLOW));
        for _ in 0..MAX_PENDING {
            queue.pending.push_back(start(CACHED));
        }
        // forced: reported with the triggered flag, not the metadata's
        let out = advance(&mut queue, 0.0, false);
        assert_eq!(out[0], (SLOW.to_owned(), false, EmoteState::EsStarted, 1));
        assert_eq!(out.len(), 1 + 2 * MAX_PENDING);

        let mut queue = EmoteReportQueue::default();
        queue.pending.push_back(start(SLOW));
        assert!(advance(&mut queue, 0.0, false).is_empty());
        assert!(advance(&mut queue, MAX_RESOLVE_SECS, false).is_empty());
        assert_eq!(
            advance(&mut queue, MAX_RESOLVE_SECS + 1.0, false),
            vec![(SLOW.to_owned(), false, EmoteState::EsStarted, 1)]
        );
    }

    #[test]
    fn moving_between_scenes_mid_emote_ends_it_there_and_starts_it_here() {
        let (a, b) = (Entity::from_raw(1), Entity::from_raw(2));
        let migrate = |queue: &mut EmoteReportQueue, targets: &[Entity]| {
            let mut out = Vec::new();
            queue.migrate(targets, |scene, urn, loops, state, ts| {
                out.push((scene, urn.to_owned(), loops, state, ts))
            });
            out
        };

        let mut queue = EmoteReportQueue::default();
        // nothing reported: nowhere to move it
        assert!(migrate(&mut queue, &[a]).is_empty());
        queue.heard(&[a]);
        assert!(queue.told.is_empty());

        queue.pending.push_back(start(CACHED));
        advance(&mut queue, 0.0, true);
        queue.heard(&[a]);
        // still in the same scene
        assert!(migrate(&mut queue, &[a]).is_empty());
        // carried from a into b
        assert_eq!(
            migrate(&mut queue, &[b]),
            vec![
                (a, CACHED.to_owned(), false, EmoteState::EsInterrupted, 2),
                (b, CACHED.to_owned(), false, EmoteState::EsStarted, 3),
            ]
        );
        assert!(migrate(&mut queue, &[b]).is_empty());

        // the stop goes to b alone, then there's nothing left to carry
        queue.pending.push_back(EmoteLifecycle::Finished);
        assert_eq!(
            advance(&mut queue, 0.0, true),
            vec![(CACHED.to_owned(), false, EmoteState::EsFinished, 4)]
        );
        queue.heard(&[b]);
        assert!(queue.told.is_empty());
        assert!(migrate(&mut queue, &[a]).is_empty());
    }
}
