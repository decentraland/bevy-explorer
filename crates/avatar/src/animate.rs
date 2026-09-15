use core::f32;
use std::{
    ops::{Deref, DerefMut},
    time::Duration,
};

use bevy::{
    animation::{graph::AnimationMask, RepeatAnimation},
    gltf::Gltf,
    math::Vec3Swizzles,
    platform::collections::{HashMap, HashSet},
    prelude::*,
    scene::InstanceId,
};
use bevy_console::ConsoleCommand;
use collectibles::{
    ext::AvatarEmotesExt, Collectible, CollectibleData, CollectibleError, CollectibleManager,
    Emote, EmoteExtraData, EmoteUrn,
};
use common::{
    rpc::{RpcCall, RpcEventSender},
    sets::SceneSets,
    structs::{
        AudioEmitter, AudioType, AvatarDynamicState, EmoteCommand, EmoteLifecycle,
        EmoteLifecycleEvent, EmoteLifecycleSource, EmoteMask, MoveKind, PlayerModifiers,
        PrimaryUser, SceneDrivenAnim, SceneDrivenAnimationFeedback,
        SceneDrivenAnimationFeedbackState,
    },
    util::TryPushChildrenEx,
};
use comms::{
    broadcast, global_crdt::ForeignPlayer, profile::CurrentUserProfile, BroadcastTarget, Transport,
};
use console::DoAddConsoleCommand;
use ipfs::IpfsAssetServer;
use scene_runner::{permissions::Permission, update_world::animation::Clips};

use crate::{process_avatar, AvatarDefinition};

#[derive(Component)]
pub struct AvatarAnimPlayer(pub Entity);

// engine-emote urns used every frame by the velocity selector; parsed once
// (EmoteUrn::new re-validates and re-allocates several times per call)
static URN_IDLE: std::sync::LazyLock<EmoteUrn> =
    std::sync::LazyLock::new(|| EmoteUrn::new("idle_male").unwrap());
static URN_JUMP: std::sync::LazyLock<EmoteUrn> =
    std::sync::LazyLock::new(|| EmoteUrn::new("jump").unwrap());
static URN_WALK: std::sync::LazyLock<EmoteUrn> =
    std::sync::LazyLock::new(|| EmoteUrn::new("walk").unwrap());
static URN_RUN: std::sync::LazyLock<EmoteUrn> =
    std::sync::LazyLock::new(|| EmoteUrn::new("run").unwrap());
static URN_DOUBLE_JUMP: std::sync::LazyLock<EmoteUrn> =
    std::sync::LazyLock::new(|| EmoteUrn::new("double_jump").unwrap());
static URN_GLIDE: std::sync::LazyLock<EmoteUrn> =
    std::sync::LazyLock::new(|| EmoteUrn::new("glide").unwrap());

// per-avatar animate state previously held in per-frame-rebuilt Local HashMaps
#[derive(Component, Default)]
pub struct AvatarAnimState {
    damped_velocity: Vec3,
    current_emote_min_velocity: f32,
}

/// The avatar graph's mask group holding the hips and legs: an upper-body emote clip is added
/// with this group masked so those bones stay with locomotion.
pub const LOWER_BODY_MASK_GROUP: u32 = 0;
const LOWER_BODY_MASK: AnimationMask = 1 << LOWER_BODY_MASK_GROUP;
/// Lowercased bone names in the lower-body group — the complement of unity's
/// `UpperBodyAvatarMask` (the `Avatar_Spine` subtree), so hips translation stays with locomotion.
pub const LOWER_BODY_BONES: [&str; 9] = [
    "avatar_hips",
    "avatar_leftupleg",
    "avatar_leftleg",
    "avatar_leftfoot",
    "avatar_lefttoebase",
    "avatar_rightupleg",
    "avatar_rightleg",
    "avatar_rightfoot",
    "avatar_righttoebase",
];

/// Upper-body emote crossfade, in and out: the triggered-emote transition time.
const MASKED_FADE_SECS: f32 = 0.2;
/// The evaluator blends every playing clip by weighted average, and the locomotion transitions
/// always sum to weight 1, so an upper-body clip at weight `s / (1 - s)` gets share `s` of the
/// bones it animates. Capped short of 1 to keep the weight finite; the remaining locomotion share
/// is invisible.
const MASKED_MAX_SHARE: f32 = 0.999;

fn masked_weight(share: f32) -> f32 {
    let share = share.min(MASKED_MAX_SHARE);
    share / (1.0 - share)
}

/// An upper-body emote (`EmoteCommand` with `EmoteMask::UpperBody`): a request that overlays
/// locomotion, kept while a full-body emote (or the glide pose) plays and resumed afterwards if it
/// loops — unity's masked-emote semantics. `animate` owns the request; `play_masked_emote` plays
/// it. Foreign avatars hold no suspended request: the wire has one emote slot and re-announces a
/// resume.
#[derive(Component, Default)]
pub struct MaskedEmote {
    request: Option<MaskedRequest>,
    /// Clip nodes by urn, added to the avatar graph with the lower body masked out.
    clips: HashMap<String, AnimationNodeIndex>,
    /// The node being driven (playing, or fading out after the request ended) and its fade
    /// fraction, the share of the upper body it gets.
    playing: Option<(AnimationNodeIndex, f32)>,
}

struct MaskedRequest {
    playback: EmotePlayback,
    /// Rendered this frame — not suspended under a full-body emote. Set by `animate`.
    active: bool,
}

impl Deref for MaskedRequest {
    type Target = EmotePlayback;

    fn deref(&self) -> &Self::Target {
        &self.playback
    }
}

impl DerefMut for MaskedRequest {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.playback
    }
}

impl MaskedEmote {
    /// An upper-body emote drives the avatar this frame (head IK yields the head to it).
    pub fn is_playing(&self) -> bool {
        self.request.as_ref().is_some_and(|request| request.active)
    }
}

pub struct AvatarAnimationPlugin;

impl Plugin for AvatarAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                (handle_trigger_emotes, broadcast_emote).before(animate),
                (animate, play_current_emote, play_masked_emote)
                    .chain()
                    .after(process_avatar),
                play_scene_driven_sounds.after(process_avatar),
            )
                .in_set(SceneSets::PostLoop),
        );
        app.add_console_command::<EmoteConsoleCommand, _>(emote_console_command);
    }
}

#[derive(Component, Default)]
pub struct LastEmoteCommand(EmoteCommand);

#[allow(clippy::type_complexity)]
fn handle_trigger_emotes(
    mut commands: Commands,
    mut emote_cmds: EventReader<RpcCall>,
    player: Query<(Entity, Option<&EmoteCommand>), With<PrimaryUser>>,
    mut perms: Permission<EmoteCommand>,
) {
    let Ok((player, maybe_prev)) = player.single() else {
        return;
    };
    let timestamp = maybe_prev
        .map(|prev| prev.timestamp + 1)
        .unwrap_or_default();

    for ev in emote_cmds.read() {
        // a stop is an empty command; `animate` reports the interruption
        let (scene, command) = match ev {
            RpcCall::TriggerEmote {
                scene,
                urn,
                r#loop,
                mask,
            } => (
                scene,
                EmoteCommand {
                    urn: urn.clone(),
                    r#loop: *r#loop,
                    timestamp,
                    mask: *mask,
                },
            ),
            RpcCall::StopEmote { scene } => (
                scene,
                EmoteCommand {
                    urn: String::new(),
                    r#loop: false,
                    timestamp,
                    mask: EmoteMask::FullBody,
                },
            ),
            _ => continue,
        };
        perms.check(
            common::structs::PermissionType::PlayEmote,
            *scene,
            command,
            None,
            false,
        );
    }

    for emote in perms.drain_success(common::structs::PermissionType::PlayEmote) {
        commands.entity(player).try_insert(emote.clone());
    }

    for _ in perms.drain_fail(common::structs::PermissionType::PlayEmote) {}
}

// Broadcast the local player's triggered emotes. Sourced from `ActiveEmote` (set by `animate` and
// stamped with the clip duration by `play_current_emote`) rather than the raw `EmoteCommand`, so a
// one-shot can carry its resolved duration and a loop is known. A start goes out on first appearance
// (or a switch to a different urn); a looping emote also sends an explicit stop when it ends —
// one-shots end on the receiver's own timer (and the Pulse server's), so they need no stop.
fn broadcast_emote(
    q: Query<(&ActiveEmote, &MaskedEmote), With<PrimaryUser>>,
    transports: Query<&Transport>,
    // the emote we last announced a start for
    mut last: Local<Option<EmotePlayback>>,
    mut count: Local<u32>,
    time: Res<Time>,
    mut senders: Local<Vec<RpcEventSender>>,
    mut subscribe_events: EventReader<RpcCall>,
) {
    // gather any event receivers
    for sender in subscribe_events.read().filter_map(|ev| match ev {
        RpcCall::SubscribePlayerExpression { sender } => Some(sender),
        _ => None,
    }) {
        senders.push(sender.clone());
    }

    // The currently-broadcastable emote: a user-triggered emote that's still playing, whose clip has
    // resolved (`duration_ms` is stamped by `play_current_emote` only once the clip loads, which is
    // also where `repeat` gets `default_repeat` folded in). Gating on it means we never announce a
    // start with a stale `repeat`/`duration` — an emote that never resolves is simply never sent.
    // Velocity-selected locomotion and scene movement anims aren't user emotes; a finished one-shot
    // has ended. The wire has one emote slot, so an upper-body emote is announced only while it's
    // actually rendered: a full-body emote over it goes out in its place, and the resume
    // afterwards is a fresh start (as unity does).
    let current = q.single().ok().and_then(|(active, masked)| {
        let resolved =
            |playback: &EmotePlayback| !playback.finished && playback.duration_ms.is_some();
        (active.source == ActiveEmoteSource::TriggeredEmote && resolved(active))
            .then(|| active.playback.clone())
            .or_else(|| {
                masked
                    .request
                    .as_ref()
                    .filter(|request| request.active && resolved(request))
                    .map(|request| request.playback.clone())
            })
    });

    let prev = last.take();
    match (&prev, &current) {
        // New emote, or a switch to a different urn or mask: announce the start.
        (p, Some(emote))
            if p.as_ref().map(|p| (&p.urn, p.mask)) != Some((&emote.urn, emote.mask)) =>
        {
            *count += 1;
            debug!(
                "sending emote start: {} {} {:?}",
                emote.urn.as_str(),
                *count,
                emote.mask
            );
            // A one-shot carries its duration (observers and the Pulse completion timer use it); a
            // looping emote omits it and is ended by the explicit stop below.
            let duration_ms = if emote.repeat {
                None
            } else {
                emote.duration_ms
            };
            broadcast(
                transports.iter(),
                BroadcastTarget::PULSE,
                false,
                comms::Emote {
                    urn: emote.urn.as_str().to_owned(),
                    incremental_id: *count,
                    timestamp: time.elapsed_secs_f64(),
                    duration_ms,
                    stopping: false,
                    mask: emote.mask,
                },
            );
            senders.retain(|sender| {
                let _ = sender.send(format!(
                    "{{ \"expressionId\": \"{}\" }}",
                    emote.urn.as_str()
                ));
                !sender.is_closed()
            });
        }
        // A looping emote ended: send an explicit stop. One-shots end on the receiver's own timer
        // (and the Pulse server's), so they need no stop and fall through to the `_` arm.
        (Some(emote), None) if emote.repeat => {
            *count += 1;
            debug!("sending emote stop: {}", emote.urn.as_str());
            broadcast(
                transports.iter(),
                BroadcastTarget::PULSE,
                false,
                comms::Emote {
                    urn: emote.urn.as_str().to_owned(),
                    incremental_id: *count,
                    timestamp: time.elapsed_secs_f64(),
                    duration_ms: None,
                    stopping: true,
                    mask: emote.mask,
                },
            );
        }
        _ => {}
    }

    // Track the current emote so the next frame can detect a change; `None` when nothing is
    // playing. Set in one place so the unchanged `_` case can't drop the state and re-fire the
    // start.
    *last = current;
}

/// Where the current ActiveEmote came from. Controls override precedence and whether
/// the `SceneDrivenAnimationFeedback` resource is updated from this emote's playback.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ActiveEmoteSource {
    /// Engine-selected from velocity heuristics (the historical default).
    #[default]
    VelocitySelected,
    /// A scene invoked triggerSceneEmote.
    TriggeredEmote,
    /// The movement scene published a `MovementAnimation` block in `AvatarMovement`.
    SceneMovementAnim,
}

/// What one of an avatar's emote slots plays: the part of a full-body `ActiveEmote` and of an
/// upper-body request that the play, broadcast and report paths share.
#[derive(Clone, Debug)]
pub struct EmotePlayback {
    pub urn: EmoteUrn,
    pub repeat: bool,
    /// Play the clip from the top: a new request, or a resume.
    pub restart: bool,
    /// A one-shot ran to its end (or the urn failed to resolve).
    pub finished: bool,
    /// Resolved clip duration in milliseconds, stamped by the play system once the clip is
    /// playing. `broadcast_emote` attaches it to a one-shot emote's start so observers know when it
    /// ends; `animate` uses its presence as the signal that `repeat` has settled.
    pub duration_ms: Option<u32>,
    /// The slot this plays in.
    pub mask: EmoteMask,
}

impl Default for EmotePlayback {
    fn default() -> Self {
        Self {
            urn: URN_IDLE.clone(),
            repeat: false,
            restart: false,
            finished: false,
            duration_ms: None,
            mask: EmoteMask::FullBody,
        }
    }
}

/// The full-body slot: the emote the avatar's body plays, whether triggered, scene-driven or
/// selected from its velocity. Derefs to its [`EmotePlayback`].
#[derive(Component)]
pub struct ActiveEmote {
    playback: EmotePlayback,
    speed: f32,
    transition_seconds: f32,
    initial_audio_mark: Option<f32>,
    /// Whether a `triggerSceneEmote` should be allowed to take over. Mirrors the movement
    /// scene's `idle` flag when `source == SceneMovementAnim`; otherwise unused.
    overridable: bool,
    /// Set by the animate system when the scene publishes a `playback_time` this frame.
    /// Consumed exactly once by `play_current_emote`.
    pending_seek: Option<f32>,
    /// Origin of the current selection; dictates override rules and feedback publishing.
    source: ActiveEmoteSource,
    /// Source path from `MovementAnimation.src`; used to populate feedback and detect
    /// cross-fade boundaries between scene-driven animations.
    scene_anim_src: Option<String>,
    /// Alternate state to play if the primary URN fails to resolve. Populated by
    /// `animate` for scene-driven selections with the velocity-based choice;
    /// `play_current_emote` swaps to it on resolution failure.
    fallback: Option<Box<ActiveEmote>>,
}

impl Default for ActiveEmote {
    fn default() -> Self {
        Self {
            playback: EmotePlayback::default(),
            speed: 1.0,
            transition_seconds: 0.2,
            initial_audio_mark: None,
            overridable: true,
            pending_seek: None,
            source: ActiveEmoteSource::VelocitySelected,
            scene_anim_src: None,
            fallback: None,
        }
    }
}

impl Deref for ActiveEmote {
    type Target = EmotePlayback;

    fn deref(&self) -> &Self::Target {
        &self.playback
    }
}

impl DerefMut for ActiveEmote {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.playback
    }
}

impl ActiveEmote {
    /// True when this represents an idle pose. For scene-driven anims the
    /// movement scene's idle flag is mirrored on `overridable`; for engine
    /// velocity-selected anims, the URN identifies it; triggered emotes
    /// (dances etc.) are never considered idle.
    pub fn is_idle(&self) -> bool {
        match self.source {
            ActiveEmoteSource::SceneMovementAnim => self.overridable,
            ActiveEmoteSource::VelocitySelected => self.urn.as_str().contains("idle"),
            ActiveEmoteSource::TriggeredEmote => false,
        }
    }

    pub fn transition_seconds(&self) -> f32 {
        self.transition_seconds
    }
}

// TODO this function is a POS
// lots of magic numbers that don't even deserve to be constants, needs reworking
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn animate(
    mut commands: Commands,
    mut avatars: Query<(
        Entity,
        &mut AvatarDynamicState,
        Option<&EmoteCommand>,
        &GlobalTransform,
        Option<(&mut ActiveEmote, &mut AvatarAnimState, &mut MaskedEmote)>,
        Option<&ForeignPlayer>,
        Option<&mut LastEmoteCommand>,
        Option<Ref<SceneDrivenAnim>>,
    )>,
    time: Res<Time>,
    player: Query<(&PrimaryUser, Option<&PlayerModifiers>)>,
    mut lifecycle: EventWriter<EmoteLifecycleEvent>,
) {
    let (gravity, jump_height) = player
        .single()
        .map(|(p, m)| m.map(|m| m.combine(p)).unwrap_or(p.clone()))
        .map(|p| (-10.0f32, p.jump_height))
        .unwrap_or((-20.0, 1.25));
    let gravity = gravity.min(-0.1);
    let jump_height = jump_height.max(0.1);

    for (
        avatar_ent,
        mut dynamic_state,
        emote,
        gt,
        active_emote,
        maybe_foreign,
        last_emote,
        maybe_scene_anim,
    ) in avatars.iter_mut()
    {
        let Some((mut active_emote, mut anim_state, mut masked)) = active_emote else {
            // `EmoteCommand` keeps whatever is already there: an emote can land in the same command
            // flush as the avatar's spawn — Pulse replays an in-progress emote right behind the
            // `PlayerJoined` — and overwriting it here would drop it before it ever played. The
            // server records that replay as sent and dedups on `(emote_id, start_seq)`, so it never
            // comes again and the avatar stays idle until the emoter retriggers.
            commands
                .entity(avatar_ent)
                .try_insert((
                    ActiveEmote::default(),
                    AvatarAnimState::default(),
                    MaskedEmote::default(),
                ))
                .try_insert_if_new((EmoteCommand::default(), LastEmoteCommand::default()));
            continue;
        };

        // calculate/store damped velocity
        let prior_velocity = anim_state.damped_velocity;
        let ratio = time.delta_secs().clamp(0.0, 0.1) / 0.1;
        let damped_velocity = dynamic_state.velocity * ratio + prior_velocity * (1.0 - ratio);
        let damped_velocity_len = damped_velocity.xz().length();
        anim_state.damped_velocity = damped_velocity;

        // `seek` (playback_time) is a one-shot, but it lives in the persistent `SceneDrivenAnim`
        // component, so honor it only on a frame the component actually arrived/changed. Remotes
        // only re-insert it on an SDA apply (so the seek fires once even while a static sender is
        // silent — e.g. a stunned hard landing); the local player's scene rewrites it every frame.
        let scene_anim_changed = maybe_scene_anim.as_ref().is_some_and(|a| a.is_changed());
        let scene_anim = maybe_scene_anim.as_deref().and_then(|a| a.active.as_ref());

        // A scene emote's urn ends `-{loop}`. That trailing token is the only loop signal
        // reaching a foreign avatar — rfc4 `PlayerEmote` has no loop field, so
        // `EmoteCommand::loop` is always false there — and it's what keeps the emote
        // looping until the wire stop instead of ending after one play.
        let parse_request = |urn: &str, r#loop: bool| {
            let parsed = EmoteUrn::new(urn).ok();
            let scene_loop = parsed
                .as_ref()
                .and_then(EmoteUrn::scene_emote)
                .is_some_and(|emote| emote.ends_with("-true"));
            (parsed, r#loop || scene_loop)
        };

        // get requested (full-body) emote; an upper-body request is tracked on `masked` below
        let (mut requested_emote, request_loop) = if let Some(EmoteCommand {
            urn,
            r#loop,
            mask: EmoteMask::FullBody,
            ..
        }) = emote
        {
            parse_request(urn, *r#loop)
        } else {
            (None, false)
        };

        let emote_changed = emote != last_emote.as_ref().map(|l| &l.0);

        let mut ended = |event: EmoteLifecycle, mask: EmoteMask| {
            lifecycle.write(EmoteLifecycleEvent {
                avatar: avatar_ent,
                event,
                source: EmoteLifecycleSource::Playback,
                mask,
            });
        };

        // The upper-body request: taken from the command when it arrives, then kept here — the
        // command slot moves on to a full-body emote (which suspends it) or a stop (which ends it).
        // The full-body path below sees a masked command as "no request", so a masked start
        // interrupts a playing full-body emote through its usual clear-on-stop.
        if emote_changed {
            match emote {
                Some(command) if command.mask == EmoteMask::UpperBody => {
                    let (urn, repeat) = parse_request(&command.urn, command.r#loop);
                    masked.request = urn.map(|urn| MaskedRequest {
                        playback: EmotePlayback {
                            urn,
                            repeat,
                            restart: true,
                            mask: EmoteMask::UpperBody,
                            ..Default::default()
                        },
                        active: false,
                    });
                    commands
                        .entity(avatar_ent)
                        .try_insert(LastEmoteCommand(command.clone()));
                }
                // `stopEmote` ends both slots
                Some(command) if command.urn.is_empty() => {
                    if masked.request.take().is_some() {
                        ended(EmoteLifecycle::Interrupted, EmoteMask::UpperBody);
                    }
                    commands
                        .entity(avatar_ent)
                        .try_insert(LastEmoteCommand(command.clone()));
                }
                // A foreign avatar's wire slot is the whole story: a full-body start replaces the
                // upper-body emote (the sender re-announces it when it resumes) and a wire stop
                // removes the command.
                _ if maybe_foreign.is_some() => masked.request = None,
                _ => (),
            }
        }

        // check expired
        if !emote_changed && Some(&active_emote.urn) != requested_emote.as_ref() {
            requested_emote = None;
        }

        // If the current animation is a non-overridable scene-driven one, silently drop
        // any triggerSceneEmote request so the movement scene retains control.
        if active_emote.source == ActiveEmoteSource::SceneMovementAnim && !active_emote.overridable
        {
            requested_emote = None;
        }

        // check / cancel requested emote
        if Some(&active_emote.urn) == requested_emote.as_ref() {
            let playing_min_vel = anim_state.current_emote_min_velocity;
            // A non-idle scene-driven animation takes precedence over a triggered emote.
            let scene_cancels = scene_anim.is_some_and(|req| !req.idle);
            // Scene-driven animations handle their own motion semantics; don't cancel on move. Only
            // the local player's emotes cancel on motion — foreign emotes persist under movement
            // until an explicit stop arrives over the wire (matching Unity), so we never infer their
            // cancellation from interpolated velocity.
            let velocity_cancels = maybe_foreign.is_none()
                && scene_anim.is_none()
                && active_emote.source != ActiveEmoteSource::SceneMovementAnim;
            if scene_cancels {
                debug!("clear on scene anim {:?}", active_emote.urn);
                requested_emote = None;
                anim_state.current_emote_min_velocity = 0.0;
                ended(EmoteLifecycle::Interrupted, EmoteMask::FullBody);
            } else if velocity_cancels && damped_velocity_len * 0.9 > playing_min_vel {
                // stop emotes on move
                debug!(
                    "clear on motion {} > {}",
                    damped_velocity_len, playing_min_vel
                );
                requested_emote = None;
                anim_state.current_emote_min_velocity = 0.0;
                ended(EmoteLifecycle::Interrupted, EmoteMask::FullBody);
            } else {
                anim_state.current_emote_min_velocity = damped_velocity_len.min(playing_min_vel);
            }

            if active_emote.finished {
                debug!("finished emoting {:?}", active_emote.urn);
                requested_emote = None;
                ended(EmoteLifecycle::Finished, EmoteMask::FullBody);
            }
        } else {
            anim_state.current_emote_min_velocity = damped_velocity_len;
            // the command was cleared under a playing emote: an explicit stop (`stopEmote`, or a
            // scene avatar's trigger cleared)
            if emote_changed
                && requested_emote.is_none()
                && active_emote.source == ActiveEmoteSource::TriggeredEmote
                && !active_emote.finished
            {
                debug!("clear on stop {:?}", active_emote.urn);
                ended(EmoteLifecycle::Interrupted, EmoteMask::FullBody);
            }
        }

        // Precompute the velocity-based selection up-front so we can use it both as the
        // fallback for scene-driven anims (in case the URN fails to resolve) and as the
        // final default when nothing else claims the avatar.
        let time_to_peak = (jump_height * -gravity * 2.0).sqrt() / -gravity;
        let just_jumped = dynamic_state.jump_time
            > (time.elapsed_secs_f64() - time_to_peak as f64 / 2.0).max(0.0);
        let (velocity_emote, velocity_move_kind) =
            if dynamic_state.move_kind == MoveKind::DoubleJump {
                // Set on foreign avatars from rfc4::Movement.jump_count >= 2 (see foreign_dynamics).
                (
                    ActiveEmote {
                        playback: EmotePlayback {
                            urn: URN_DOUBLE_JUMP.clone(),
                            ..Default::default()
                        },
                        speed: 1.0,
                        transition_seconds: 0.1,
                        ..Default::default()
                    },
                    MoveKind::DoubleJump,
                )
            } else if dynamic_state.move_kind == MoveKind::Glide {
                // Set on foreign avatars from rfc4::Movement.glide_state ∈ {OPENING_PROP, GLIDING}.
                // Frozen at the neutral (straight) pose since we have no tilt input available.
                (
                    ActiveEmote {
                        playback: EmotePlayback {
                            urn: URN_GLIDE.clone(),
                            repeat: true,
                            ..Default::default()
                        },
                        speed: 0.0,
                        transition_seconds: 0.1,
                        pending_seek: Some(2.0 / 24.0),
                        ..Default::default()
                    },
                    MoveKind::Glide,
                )
            } else if dynamic_state.ground_height > 0.2
                || (dynamic_state.velocity.y > 0.0 && just_jumped)
            {
                let move_kind = if just_jumped {
                    MoveKind::Jump
                } else {
                    MoveKind::Falling
                };
                (
                    ActiveEmote {
                        playback: EmotePlayback {
                            urn: URN_JUMP.clone(),
                            repeat: true,
                            restart: dynamic_state.jump_time
                                > time.elapsed_secs_f64() - time.delta_secs_f64(),
                            ..Default::default()
                        },
                        speed: time_to_peak.recip() * 0.5,
                        transition_seconds: 0.1,
                        initial_audio_mark: if !just_jumped { Some(0.1) } else { None },
                        ..Default::default()
                    },
                    move_kind,
                )
            } else if active_emote.urn == *URN_JUMP && !active_emote.finished {
                (
                    ActiveEmote {
                        playback: EmotePlayback {
                            urn: URN_JUMP.clone(),
                            ..Default::default()
                        },
                        speed: 1.5,
                        transition_seconds: 0.1,
                        initial_audio_mark: Some(0.1),
                        ..Default::default()
                    },
                    dynamic_state.move_kind,
                )
            } else {
                let directional_velocity_len =
                    (damped_velocity * (Vec3::X + Vec3::Z)).dot(gt.forward().as_vec3());
                if damped_velocity_len.abs() > 0.1 {
                    if damped_velocity_len.abs() <= 2.6 {
                        (
                            ActiveEmote {
                                playback: EmotePlayback {
                                    urn: URN_WALK.clone(),
                                    repeat: true,
                                    ..Default::default()
                                },
                                speed: directional_velocity_len / 1.5,
                                transition_seconds: 0.4,
                                ..Default::default()
                            },
                            MoveKind::Walk,
                        )
                    } else {
                        (
                            ActiveEmote {
                                playback: EmotePlayback {
                                    urn: URN_RUN.clone(),
                                    repeat: true,
                                    ..Default::default()
                                },
                                speed: directional_velocity_len / 4.5,
                                transition_seconds: 0.4,
                                ..Default::default()
                            },
                            MoveKind::Jog,
                        )
                    }
                } else {
                    (
                        ActiveEmote {
                            playback: EmotePlayback {
                                urn: URN_IDLE.clone(),
                                repeat: true,
                                ..Default::default()
                            },
                            speed: 1.0,
                            transition_seconds: 0.4,
                            ..Default::default()
                        },
                        MoveKind::Idle,
                    )
                }
            };

        // play requested emote
        *active_emote = if let Some(requested_emote) = requested_emote {
            if emote_changed {
                dynamic_state.move_kind = MoveKind::Emote;
            }
            commands
                .entity(avatar_ent)
                .try_insert(LastEmoteCommand(emote.unwrap().clone()));
            ActiveEmote {
                playback: EmotePlayback {
                    urn: requested_emote,
                    repeat: request_loop,
                    restart: emote_changed,
                    ..Default::default()
                },
                source: ActiveEmoteSource::TriggeredEmote,
                ..Default::default()
            }
        } else if let Some(scene_anim_req) =
            scene_anim.and_then(|req| EmoteUrn::new(req.urn.as_str()).ok().map(|urn| (req, urn)))
        {
            let (req, urn) = scene_anim_req;
            dynamic_state.move_kind = if req.idle {
                MoveKind::Idle
            } else {
                MoveKind::Walk
            };
            // Detect anim change via URN (stable across local/remote sources) rather than src,
            // which is empty for requests received over the network.
            let is_new_anim = active_emote.source != ActiveEmoteSource::SceneMovementAnim
                || active_emote.urn != urn;
            ActiveEmote {
                playback: EmotePlayback {
                    urn,
                    repeat: req.r#loop,
                    restart: is_new_anim,
                    ..Default::default()
                },
                speed: req.speed,
                transition_seconds: req.transition_seconds,
                initial_audio_mark: None,
                overridable: req.idle,
                pending_seek: if scene_anim_changed { req.seek } else { None },
                source: ActiveEmoteSource::SceneMovementAnim,
                scene_anim_src: Some(req.src.clone()),
                fallback: Some(Box::new(velocity_emote)),
            }
        } else {
            dynamic_state.move_kind = velocity_move_kind;
            velocity_emote
        };

        // The upper-body request overlays locomotion but not a full-body emote, nor the glide
        // pose (which owns the arms). A looping one waits that out and resumes from the top; a
        // one-shot is dropped, as in unity.
        if let Some(request) = masked.request.as_mut() {
            let suspended = active_emote.source == ActiveEmoteSource::TriggeredEmote
                || active_emote.urn == *URN_GLIDE;
            if suspended {
                if request.active && !request.repeat {
                    debug!("dropping suspended one-shot {:?}", request.urn);
                    masked.request = None;
                    ended(EmoteLifecycle::Interrupted, EmoteMask::UpperBody);
                } else {
                    request.active = false;
                    request.restart = true;
                }
            } else if request.finished {
                debug!("finished masked emoting {:?}", request.urn);
                masked.request = None;
                ended(EmoteLifecycle::Finished, EmoteMask::UpperBody);
            } else {
                request.active = true;
            }
        }
    }
}

struct SpawnedExtras {
    urn: EmoteUrn,
    scene: Option<(Entity, InstanceId)>,
    scene_initialized: bool,
    audio: Option<(Entity, f32)>,
    clip: Option<(AnimationNodeIndex, Handle<AnimationGraph>)>,
    // Intended clip-start wall-time (`elapsed_secs - playtime`) captured when a seek was
    // requested on the frame the prop scene was spawned. Spawning a prop defers the
    // emote's play (we `continue` until the instance is ready), so a one-shot
    // `playback_time` from the scene would otherwise be gone before it's applied. Storing
    // the *start* time (rather than the raw seek) lets the deferred play resume at
    // `elapsed_secs - start` — i.e. advanced by however long the prop took to load — so a
    // slow load doesn't resume stale. Cleared when the avatar + prop finally play.
    deferred_start: Option<f64>,
}

impl SpawnedExtras {
    pub fn new(urn: EmoteUrn) -> Self {
        Self {
            urn,
            scene: None,
            scene_initialized: false,
            audio: None,
            clip: None,
            deferred_start: None,
        }
    }
}

/// Set up a freshly spawned prop scene instance: hide the emote gltf's avatar reference meshes
/// like unity does, turn the root nodes to face the avatar, and show the wrapper.
fn init_prop_instance(
    commands: &mut Commands,
    scene_spawner: &SceneSpawner,
    instance: InstanceId,
    wrapper: Entity,
    prop_details: &Query<(Option<&Name>, &Transform, &ChildOf)>,
) {
    for spawned_ent in scene_spawner.iter_instance_entities(instance) {
        if let Ok((maybe_name, transform, parent)) = prop_details.get(spawned_ent) {
            // hide stuff like unity
            // what a mess
            if let Some(name) = maybe_name.map(Name::as_str).map(str::to_ascii_lowercase) {
                if name.contains("_reference")
                    || name.ends_with("_basemesh")
                    || name.starts_with("m_mask_")
                {
                    commands.entity(spawned_ent).try_insert(Visibility::Hidden);
                    debug!("hiding emote prop `{name}` due to name");
                }
            }

            if parent.parent() == wrapper {
                // children of root nodes -> rotate
                let mut rotated = *transform;
                rotated.rotate_around(Vec3::ZERO, Quat::from_rotation_y(std::f32::consts::PI));
                commands.entity(spawned_ent).try_insert(rotated);
            }
        }
    }
    commands.entity(wrapper).try_insert(Visibility::Inherited);
}

/// The prop scene's animation players, each given a graph holding the emote's prop clip (built on
/// first use into `cached`). `has_graph` says whether an entity is a player and already has a
/// graph.
fn prop_players(
    commands: &mut Commands,
    scene_spawner: &SceneSpawner,
    graphs: &mut Assets<AnimationGraph>,
    instance: InstanceId,
    prop_clip: Handle<AnimationClip>,
    cached: &mut Option<(AnimationNodeIndex, Handle<AnimationGraph>)>,
    has_graph: impl Fn(Entity) -> Option<bool>,
) -> Option<(Vec<Entity>, AnimationNodeIndex)> {
    let clip = cached.get_or_insert_with(|| {
        let (graph, ix) = AnimationGraph::from_clip(prop_clip);
        (ix, graphs.add(graph))
    });

    let players = scene_spawner
        .iter_instance_entities(instance)
        .filter(|ent| match has_graph(*ent) {
            Some(has_graph) => {
                if !has_graph {
                    commands
                        .entity(*ent)
                        .try_insert(AnimationGraphHandle(clip.1.clone()));
                }
                true
            }
            None => false,
        })
        .collect::<Vec<_>>();
    (!players.is_empty()).then_some((players, clip.0))
}

/// The next sound the emote fires after `last_audio_mark` seconds of playback (`NEG_INFINITY`
/// before the first), wrapping into the next cycle of a repeating emote. `Err` while a sound is
/// still loading.
fn next_emote_sound(
    emote: &Emote,
    sounds: &Assets<bevy_kira_audio::AudioSource>,
    last_audio_mark: f32,
    clip_duration: f32,
    repeat: bool,
) -> Result<Option<(f32, Handle<bevy_kira_audio::AudioSource>)>, CollectibleError> {
    let completions = (last_audio_mark / clip_duration).floor();

    // get next time to play a sound, with a lot of messing around for inf values
    let sound = emote
        .audio(
            sounds,
            if last_audio_mark.is_finite() {
                last_audio_mark % clip_duration
            } else {
                last_audio_mark
            },
        )?
        .map(|(t, s)| {
            if last_audio_mark.is_finite() {
                (t + completions * clip_duration, s)
            } else {
                (t, s)
            }
        });
    debug!(
        "audio with mark {last_audio_mark} -> {:?}",
        sound.as_ref().map(|(t, _)| t)
    );
    if sound.is_none() && repeat {
        Ok(emote
            .audio(sounds, f32::NEG_INFINITY)?
            .map(|(play_time, s)| (play_time + clip_duration * (completions + 1.0), s)))
    } else {
        Ok(sound)
    }
}

/// Play `sound` from the avatar's emote emitter, spawned under `player_ent` on first use and
/// tracked in `audio` with the playback mark it fired at.
fn emit_emote_sound(
    commands: &mut Commands,
    emitters: &mut Query<&mut AudioEmitter>,
    audio: &mut Option<(Entity, f32)>,
    player_ent: Entity,
    sound: Handle<bevy_kira_audio::AudioSource>,
    elapsed: f32,
) {
    if let Some(mut existing_emitter) = audio.as_ref().and_then(|(e, _)| emitters.get_mut(*e).ok())
    {
        *existing_emitter = AudioEmitter {
            handle: sound,
            ty: AudioType::Avatar,
            ..Default::default()
        };
        audio.as_mut().unwrap().1 = elapsed;
    } else {
        let audio_entity = commands
            .spawn((
                Transform::default(),
                Visibility::default(),
                AudioEmitter {
                    handle: sound,
                    ty: AudioType::Avatar,
                    ..Default::default()
                },
            ))
            .id();

        if let Ok(mut commands) = commands.get_entity(player_ent) {
            commands.try_push_children(&[audio_entity]);
        }

        *audio = Some((audio_entity, elapsed));
    }
}

/// Drop an emote's prop scene and audio emitter.
fn despawn_extras(
    commands: &mut Commands,
    scene_spawner: &mut SceneSpawner,
    extras: SpawnedExtras,
) {
    if let Some((wrapper, scene)) = extras.scene {
        scene_spawner.despawn_instance(scene);
        if let Ok(mut commands) = commands.get_entity(wrapper) {
            commands.despawn();
        }
    }
    if let Some((audio_ent, _)) = extras.audio {
        if let Ok(mut commands) = commands.get_entity(audio_ent) {
            commands.despawn();
        }
    }
}

enum Outcome {
    Ready,
    Loading,
    Failed,
}

/// Resolve an emote urn to a loaded avatar clip for `bodyshape`: a scene emote's gltf is fetched
/// through its scene's content (and registered as a builtin collectible on first sight), a
/// wearable emote through the collectible manager.
fn resolve_emote(
    urn: &EmoteUrn,
    bodyshape: &str,
    emote_loader: &mut CollectibleManager<Emote>,
    gltfs: &mut Assets<Gltf>,
    ipfas: &IpfsAssetServer,
    cached_gltf_handles: &mut HashSet<Handle<Gltf>>,
) -> Outcome {
    if let Some(scene_emote) = urn.scene_emote() {
        debug!("got {scene_emote:?}");
        let mut split = scene_emote.split('-').peekable();
        // take_hash reads a hash, recombining "b64-<payload>" back into one
        // token because we used '-' as the separator and b64 hashes also
        // contain '-'. for non-b64 hashes it just takes the next token.
        let take_hash =
            |split: &mut std::iter::Peekable<std::str::Split<'_, char>>| -> Option<String> {
                let first = split.next()?;
                if first == "b64" {
                    let tail = split.next()?;
                    Some(format!("b64-{tail}"))
                } else {
                    Some(first.to_owned())
                }
            };
        let Some(scene_hash) = take_hash(&mut split) else {
            debug!("failed to split scene emote {scene_emote:?}");
            return Outcome::Failed;
        };
        let Some(hash) = take_hash(&mut split) else {
            debug!("failed to split scene emote {scene_emote:?}");
            return Outcome::Failed;
        };

        if emote_loader.get_representation(urn, bodyshape).is_err() {
            // load the gltf through the scene's modifier context so b64
            // hashes (local preview / portable) resolve to the scene's
            // origin rather than the realm content URL.
            let handle = ipfas.load_scene_content_hash::<Gltf>(&scene_hash, &hash);
            let gltf = match gltfs.get_mut(handle.id()) {
                Some(gltf) => {
                    cached_gltf_handles.remove(&handle);
                    gltf
                }
                None => {
                    cached_gltf_handles.insert(handle);
                    return Outcome::Loading;
                }
            };

            // fix up the gltf if possible/required
            if !gltf.named_animations.any_avatar_emote() {
                let Some(anim) = gltf.animations.first() else {
                    warn!("scene emote has no animations");
                    return Outcome::Failed;
                };

                gltf.named_animations.insert("_Avatar".into(), anim.clone());
            }

            // add repr
            emote_loader.add_builtin(
                urn.clone(),
                Collectible {
                    representations: HashMap::from_iter([(
                        bodyshape.to_owned(),
                        Emote {
                            gltf: handle,
                            default_repeat: false,
                            sound: Vec::default(),
                        },
                    )]),
                    data: CollectibleData::<Emote> {
                        hash: hash.to_owned(),
                        urn: urn.as_str().to_owned(),
                        thumbnail: "embedded://images/redx.png".to_owned(),
                        available_representations: HashSet::from_iter([bodyshape.to_owned()]),
                        name: urn.to_string(),
                        description: urn.to_string(),
                        extra_data: EmoteExtraData::default(),
                    },
                },
            );
        }
    }

    match emote_loader.get_representation(urn, bodyshape) {
        Ok(emote) => match emote.avatar_animation(gltfs) {
            Ok(Some(_)) => Outcome::Ready,
            Err(e) => {
                debug!("animation error: {:?}", e);
                Outcome::Loading
            }
            Ok(None) => {
                debug!("{} -> no clip", urn);
                Outcome::Failed
            }
        },
        Err(CollectibleError::Loading) => {
            debug!("{} -> loading", urn);
            Outcome::Loading
        }
        Err(e) => {
            debug!("{} -> {:?}", urn, e);
            Outcome::Failed
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn play_current_emote(
    mut commands: Commands,
    mut q: Query<(
        Entity,
        &mut ActiveEmote,
        &AvatarAnimPlayer,
        &Children,
        Option<&PrimaryUser>,
    )>,
    definitions: Query<&AvatarDefinition>,
    mut emote_loader: CollectibleManager<Emote>,
    mut gltfs: ResMut<Assets<Gltf>>,
    mut players: Query<(
        &mut AnimationPlayer,
        Option<&mut AnimationTransitions>,
        Option<&mut Clips>,
        Option<&AnimationGraphHandle>,
    )>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut playing: Local<HashMap<Entity, EmoteUrn>>,
    ipfas: IpfsAssetServer,
    mut cached_gltf_handles: Local<HashSet<Handle<Gltf>>>,
    // spawned_extras: prop/audio for the current clip. retiring_extras: prop kept alive
    // across a clip change until its replacement is visible (or the new clip has no
    // prop), so a prop carried by both clips (e.g. an embedded glider) doesn't blink out
    // while the new scene loads — (wrapper entity, scene instance) keyed by avatar.
    // Grouped as a tuple to stay within Bevy's per-system param limit.
    (mut spawned_extras, mut retiring_extras): (
        Local<HashMap<Entity, SpawnedExtras>>,
        Local<HashMap<Entity, (Entity, InstanceId)>>,
    ),
    mut scene_spawner: ResMut<SceneSpawner>,
    (sounds, anim_clips, time): (
        Res<Assets<bevy_kira_audio::AudioSource>>,
        Res<Assets<AnimationClip>>,
        Res<Time>,
    ),
    mut emitters: Query<&mut AudioEmitter>,
    prop_details: Query<(Option<&Name>, &Transform, &ChildOf)>,
    (mut feedback, mut frozen_feedback): (
        ResMut<SceneDrivenAnimationFeedback>,
        Local<Option<SceneDrivenAnimationFeedbackState>>,
    ),
) {
    let mut prior_playing = std::mem::take(&mut *playing);
    let mut prev_spawned_extras = std::mem::take(&mut *spawned_extras);

    for (entity, mut active_emote, target_entity, children, maybe_primary) in q.iter_mut() {
        debug!("emote {}", active_emote.urn);
        let Some(definition) = children.iter().flat_map(|c| definitions.get(c).ok()).next() else {
            warn!("no definition");
            continue;
        };

        // clean up old extras
        if let Some(extras) = prev_spawned_extras.remove(&entity) {
            if extras.urn != active_emote.urn {
                // Defer despawning the old prop scene until its replacement is visible
                // (or we learn the new clip has no prop) to avoid a one-frame gap while
                // the new scene loads. Despawn any already-retiring prop first, so a
                // rapid run of switches can't accumulate more than one.
                if let Some((wrapper, scene)) = extras.scene {
                    if let Some((old_wrapper, old_scene)) =
                        retiring_extras.insert(entity, (wrapper, scene))
                    {
                        scene_spawner.despawn_instance(old_scene);
                        if let Ok(mut commands) = commands.get_entity(old_wrapper) {
                            commands.despawn();
                        }
                    }
                }

                if let Some((audio_ent, _)) = extras.audio.as_ref() {
                    if let Ok(mut commands) = commands.get_entity(*audio_ent) {
                        commands.despawn();
                    }
                }
            } else {
                spawned_extras.insert(entity, extras);
            }
        }

        let ent = target_entity.0;
        let bodyshape = &definition.body_shape;

        // Resolve the URN. On permanent failure, if a fallback is present (populated by
        // `animate` for scene-driven anims with the velocity-based choice), swap to it
        // and retry. The swap persists so downstream `SceneDrivenAnimationFeedback`
        // publishing reports the fallback source, not the failed scene-driven one.
        let outcome = loop {
            match resolve_emote(
                &active_emote.urn,
                bodyshape,
                &mut emote_loader,
                &mut gltfs,
                &ipfas,
                &mut cached_gltf_handles,
            ) {
                Outcome::Failed => {
                    if let Some(fb) = active_emote.fallback.take() {
                        *active_emote = *fb;
                        continue;
                    }
                    break Outcome::Failed;
                }
                outcome => break outcome,
            }
        };

        match outcome {
            Outcome::Ready => {}
            Outcome::Loading => continue,
            Outcome::Failed => {
                active_emote.finished = true;
                continue;
            }
        }

        let emote = match emote_loader.get_representation(&active_emote.urn, bodyshape.as_str()) {
            Ok(emote) => emote,
            _ => continue,
        };
        active_emote.repeat |= emote.default_repeat;

        let clip = match emote.avatar_animation(&gltfs) {
            Ok(Some(clip)) => clip,
            _ => continue,
        };

        // extract props and prop anim
        let mut prop_player_and_clip = None;
        if let Ok(Some(props)) = emote.prop_scene(&gltfs) {
            debug!("got props");
            if let Some(extras) = spawned_extras.get_mut(&entity) {
                let Some((wrapper, instance)) = extras.scene else {
                    continue;
                };

                if !scene_spawner.instance_is_ready(instance) {
                    continue;
                }

                if !extras.scene_initialized {
                    init_prop_instance(
                        &mut commands,
                        &scene_spawner,
                        instance,
                        wrapper,
                        &prop_details,
                    );
                    extras.scene_initialized = true;
                    // Replacement is up — and posed this same frame thanks to the
                    // thread_animation_graphs ordering fix in the engine — so drop the
                    // prop we kept alive across the switch. The overlap covers the new
                    // prop's load time; the engine fix covers the would-be bind-pose frame.
                    if let Some((old_wrapper, old_scene)) = retiring_extras.remove(&entity) {
                        scene_spawner.despawn_instance(old_scene);
                        if let Ok(mut commands) = commands.get_entity(old_wrapper) {
                            commands.despawn();
                        }
                    }
                }

                if let Ok(Some(prop_clip)) = emote.prop_anim(&gltfs) {
                    prop_player_and_clip = prop_players(
                        &mut commands,
                        &scene_spawner,
                        &mut graphs,
                        instance,
                        prop_clip,
                        &mut extras.clip,
                        |ent| players.get(ent).ok().map(|(_, _, _, g)| g.is_some()),
                    );
                }
            } else {
                let wrapper = commands
                    .spawn((Transform::default(), Visibility::Hidden, ChildOf(entity)))
                    .id();
                let scene = scene_spawner.spawn_as_child(props, wrapper);
                let extras = spawned_extras
                    .entry(entity)
                    .or_insert_with(|| SpawnedExtras::new(active_emote.urn.clone()));
                extras.scene = Some((wrapper, scene));
                // Carry any seek requested this frame across the deferred play (below), so
                // a one-shot scene `playback_time` isn't lost while the prop loads. Stash
                // the intended clip-start time so the deferred play resumes advanced by
                // the load duration rather than at the (now-stale) requested value.
                extras.deferred_start = active_emote
                    .pending_seek
                    .map(|seek| time.elapsed_secs_f64() - seek as f64);
                continue;
            }
        }

        // New clip definitively has no prop — drop any prop kept alive across the switch
        // (e.g. glide ended). `Err(Loading)` is left pending so we keep showing the old
        // prop until we know.
        if matches!(emote.prop_scene(&gltfs), Ok(None)) {
            if let Some((old_wrapper, old_scene)) = retiring_extras.remove(&entity) {
                scene_spawner.despawn_instance(old_scene);
                if let Ok(mut commands) = commands.get_entity(old_wrapper) {
                    commands.despawn();
                }
            }
        }

        let default_audio_mark = active_emote.initial_audio_mark.unwrap_or(f32::NEG_INFINITY);
        let last_audio_mark = if active_emote.restart {
            default_audio_mark
        } else {
            spawned_extras
                .get(&entity)
                .and_then(|extras| extras.audio.as_ref())
                .map(|(_, mark)| *mark)
                .unwrap_or(default_audio_mark)
        };

        let Some(clip_duration) = anim_clips.get(&clip).map(|c| c.duration()) else {
            continue;
        };
        let Ok(sound) = next_emote_sound(
            emote,
            &sounds,
            last_audio_mark,
            clip_duration,
            active_emote.repeat,
        ) else {
            continue;
        };

        let urn_was_playing = prior_playing.get(&ent) == Some(&active_emote.urn);
        let play = |transitions: Option<Mut<AnimationTransitions>>,
                    player: &mut AnimationPlayer,
                    clip_ix: AnimationNodeIndex,
                    active_emote: &ActiveEmote,
                    pending_seek: Option<f32>|
         -> f32 {
            let active_animation = if !urn_was_playing || active_emote.restart {
                let active_animation = match transitions {
                    Some(mut t) => t.play(
                        player,
                        clip_ix,
                        Duration::from_secs_f32(active_emote.transition_seconds),
                    ),
                    None => player.start(clip_ix),
                };
                debug!("starting clip {:?}", clip_ix);
                active_animation.seek_to(0.0);
                Some(active_animation)
            } else {
                player
                    .playing_animations_mut()
                    .find(|(nix, _)| **nix == clip_ix)
                    .map(|(_, anim)| anim)
            };

            if let Some(active_animation) = active_animation {
                if active_emote.repeat {
                    active_animation.repeat();
                } else {
                    active_animation.set_repeat(RepeatAnimation::Never);
                }

                // println!("active weight {}", active_animation.weight());
                active_animation.set_speed(active_emote.speed);

                if let Some(seek) = pending_seek {
                    // `replay()` in bevy_animation resets `seek_time` to 0.0
                    // (among other state), so it must run BEFORE `seek_to`
                    // or the seek is clobbered. We still call it so a non-
                    // looping clip that has completed can be restarted by a
                    // new seek.
                    active_animation.replay();
                    active_animation.seek_to(seek.clamp(0.0, clip_duration));
                }

                // nasty hack for falling animation
                if active_emote.urn.as_str() == "urn:decentraland:off-chain:base-emotes:jump"
                    && active_animation.seek_time() >= 0.4
                    && active_emote.repeat
                {
                    active_animation.set_speed(active_emote.speed * 0.125);
                }
                if active_emote.urn.as_str() == "urn:decentraland:off-chain:base-emotes:jump"
                    && active_animation.seek_time() >= 0.5833
                    && active_emote.repeat
                {
                    active_animation.seek_to(0.5833);
                    active_animation.set_speed(0.0);
                }

                active_animation.seek_time() + active_animation.completions() as f32 * clip_duration
            } else {
                0.0
            }
        };

        let Ok((mut player, transitions, clips, graph)) = players.get_mut(ent) else {
            debug!("no player");
            active_emote.finished = true;
            continue;
        };

        let mut clips = clips.unwrap();
        // look up by &str first — the entry api would allocate a String per call
        let clip_ix = match clips.named.get(active_emote.urn.as_str()) {
            Some((ix, _)) => *ix,
            None => {
                debug!("adding clip");
                let ix = match graph.and_then(|graph| graphs.get_mut(graph)) {
                    Some(graph) => graph.add_clip(clip, 1.0, graph.root),
                    None => AnimationNodeIndex::new(u32::MAX as usize),
                };
                clips.named.insert(active_emote.urn.to_string(), (ix, 0.0));
                ix
            }
        };

        // Prefer a seek requested this frame; otherwise apply one stashed when the prop
        // was spawned (deferred a frame), advanced by the load duration via the stashed
        // start time, so the avatar + prop pick up in phase regardless of load time.
        let pending_seek = active_emote.pending_seek.take().or_else(|| {
            spawned_extras
                .get_mut(&entity)
                .and_then(|extras| extras.deferred_start.take())
                .map(|start| (time.elapsed_secs_f64() - start) as f32)
        });
        let elapsed = play(
            transitions,
            &mut player,
            clip_ix,
            &active_emote,
            pending_seek,
        );
        // reset audio mark if we've rewound (jump hacks again)
        if let Some(mark) = spawned_extras
            .get_mut(&entity)
            .and_then(|extras| extras.audio.as_mut())
            .map(|a| &mut a.1)
        {
            if elapsed < *mark {
                *mark = elapsed;
            }
        }

        if !active_emote.finished && player.all_finished() {
            active_emote.finished = true;
        }

        if let Some((prop_player_ents, clip_ix)) = prop_player_and_clip {
            for ent in prop_player_ents {
                if let Ok((mut player, transitions, _, _)) = players.get_mut(ent) {
                    play(
                        transitions,
                        &mut player,
                        clip_ix,
                        &active_emote,
                        pending_seek,
                    );
                }
            }
        }

        // Stamped for every avatar: `animate` reads it back as "this emote resolved, so `repeat`
        // now includes the collectible's `default_repeat`" before reporting the emote to scenes.
        active_emote.duration_ms = Some((clip_duration * 1000.0) as u32);

        if maybe_primary.is_some() {
            match active_emote.source {
                ActiveEmoteSource::SceneMovementAnim => {
                    let loops = elapsed / clip_duration;
                    let playback_time = if active_emote.repeat && clip_duration > 0.0 {
                        elapsed - loops.floor() * clip_duration
                    } else {
                        elapsed.min(clip_duration)
                    };
                    let loop_count = if clip_duration > 0.0 {
                        loops.floor().max(0.0) as u32
                    } else {
                        0
                    };
                    let state = SceneDrivenAnimationFeedbackState {
                        src: active_emote.scene_anim_src.clone().unwrap_or_default(),
                        r#loop: active_emote.repeat,
                        speed: active_emote.speed,
                        idle: active_emote.overridable,
                        playback_time,
                        duration: clip_duration,
                        loop_count,
                    };
                    *frozen_feedback = Some(state.clone());
                    feedback.state = Some(state);
                }
                ActiveEmoteSource::TriggeredEmote => {
                    feedback.state = frozen_feedback.clone();
                }
                ActiveEmoteSource::VelocitySelected => {
                    *frozen_feedback = None;
                    feedback.state = None;
                }
            }
        }

        active_emote.restart = false;

        if let Some((play_time, sound)) = sound {
            if elapsed >= play_time {
                debug!("duration {}", clip_duration);
                debug!("play {:?} @ {}>{}", sound.path(), elapsed, play_time);
                let extras = spawned_extras
                    .entry(entity)
                    .or_insert_with(|| SpawnedExtras::new(active_emote.urn.clone()));
                emit_emote_sound(
                    &mut commands,
                    &mut emitters,
                    &mut extras.audio,
                    ent,
                    sound,
                    elapsed,
                );
            }
        }

        // move the prior entry back when unchanged to avoid a per-frame clone
        playing.insert(
            ent,
            match prior_playing.remove(&ent) {
                Some(prev) if prev == active_emote.urn => prev,
                _ => active_emote.urn.clone(),
            },
        );
    }
}

/// Play each avatar's upper-body emote request over whatever `play_current_emote` has going: the
/// clip is added to the avatar graph with the lower body masked out and driven on the same player,
/// outside the locomotion transitions, at a weight that gives it (nearly) the whole upper body —
/// see `masked_weight`. Faded in on start and out when the request ends or is suspended.
///
/// The emote's prop and audio come along like a full-body emote's, and go the moment the request
/// ends or is suspended (unity drops them too; a resume brings them back from the top).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn play_masked_emote(
    mut commands: Commands,
    mut q: Query<(Entity, &mut MaskedEmote, &AvatarAnimPlayer, &Children)>,
    definitions: Query<&AvatarDefinition>,
    mut emote_loader: CollectibleManager<Emote>,
    mut gltfs: ResMut<Assets<Gltf>>,
    mut players: Query<(&mut AnimationPlayer, Option<&AnimationGraphHandle>)>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    ipfas: IpfsAssetServer,
    mut cached_gltf_handles: Local<HashSet<Handle<Gltf>>>,
    (anim_clips, sounds, time): (
        Res<Assets<AnimationClip>>,
        Res<Assets<bevy_kira_audio::AudioSource>>,
        Res<Time>,
    ),
    // prop/audio for each avatar's rendered request
    mut spawned_extras: Local<HashMap<Entity, SpawnedExtras>>,
    mut scene_spawner: ResMut<SceneSpawner>,
    mut emitters: Query<&mut AudioEmitter>,
    prop_details: Query<(Option<&Name>, &Transform, &ChildOf)>,
) {
    let mut prev_spawned_extras = std::mem::take(&mut *spawned_extras);

    for (entity, mut masked, target_entity, children) in q.iter_mut() {
        let masked = &mut *masked;
        let ent = target_entity.0;
        let Ok((_, graph)) = players.get(ent) else {
            continue;
        };
        let graph = graph.map(|graph| graph.0.clone());

        // the request rendered this frame; extras of any other (ended, suspended, replaced) go
        let rendered = masked
            .request
            .as_mut()
            .filter(|request| request.active && !request.finished);
        if let Some(extras) = prev_spawned_extras.remove(&entity) {
            if rendered
                .as_ref()
                .is_some_and(|request| request.urn == extras.urn)
            {
                spawned_extras.insert(entity, extras);
            } else {
                despawn_extras(&mut commands, &mut scene_spawner, extras);
            }
        }

        // the share of the upper body the request wants this frame: 1 while it plays, 0 once it
        // ends or is suspended, unchanged while its clip loads
        let current_share = masked.playing.map(|(_, share)| share).unwrap_or(0.0);
        let target_share = 'request: {
            let Some(request) = rendered else {
                break 'request 0.0;
            };
            let Some(definition) = children.iter().flat_map(|c| definitions.get(c).ok()).next()
            else {
                break 'request current_share;
            };
            let bodyshape = &definition.body_shape;

            match resolve_emote(
                &request.urn,
                bodyshape,
                &mut emote_loader,
                &mut gltfs,
                &ipfas,
                &mut cached_gltf_handles,
            ) {
                Outcome::Ready => (),
                Outcome::Loading => break 'request current_share,
                Outcome::Failed => {
                    request.finished = true;
                    break 'request 0.0;
                }
            }
            let Ok(emote) = emote_loader.get_representation(&request.urn, bodyshape) else {
                break 'request current_share;
            };
            request.repeat |= emote.default_repeat;
            let Ok(Some(clip)) = emote.avatar_animation(&gltfs) else {
                break 'request current_share;
            };
            let Some(clip_duration) = anim_clips.get(&clip).map(|c| c.duration()) else {
                break 'request current_share;
            };
            request.duration_ms = Some((clip_duration * 1000.0) as u32);

            // spawn the prop and wait for it, so avatar and prop start together
            let mut prop_player_and_clip = None;
            if let Ok(Some(props)) = emote.prop_scene(&gltfs) {
                match spawned_extras.get_mut(&entity) {
                    Some(extras) => {
                        let Some((wrapper, instance)) = extras.scene else {
                            break 'request current_share;
                        };
                        if !scene_spawner.instance_is_ready(instance) {
                            break 'request current_share;
                        }
                        if !extras.scene_initialized {
                            init_prop_instance(
                                &mut commands,
                                &scene_spawner,
                                instance,
                                wrapper,
                                &prop_details,
                            );
                            extras.scene_initialized = true;
                        }
                        if let Ok(Some(prop_clip)) = emote.prop_anim(&gltfs) {
                            prop_player_and_clip = prop_players(
                                &mut commands,
                                &scene_spawner,
                                &mut graphs,
                                instance,
                                prop_clip,
                                &mut extras.clip,
                                |ent| players.get(ent).ok().map(|(_, g)| g.is_some()),
                            );
                        }
                    }
                    None => {
                        let wrapper = commands
                            .spawn((Transform::default(), Visibility::Hidden, ChildOf(entity)))
                            .id();
                        let scene = scene_spawner.spawn_as_child(props, wrapper);
                        let mut extras = SpawnedExtras::new(request.urn.clone());
                        extras.scene = Some((wrapper, scene));
                        spawned_extras.insert(entity, extras);
                        break 'request current_share;
                    }
                }
            }

            let Some(graph) = graph.as_ref().and_then(|graph| graphs.get_mut(graph)) else {
                break 'request current_share;
            };
            let clip_ix = *masked
                .clips
                .entry(request.urn.to_string())
                .or_insert_with(|| {
                    debug!("adding masked clip {}", request.urn);
                    graph.add_clip_with_mask(clip, LOWER_BODY_MASK, 1.0, graph.root)
                });

            let Ok((mut player, _)) = players.get_mut(ent) else {
                break 'request current_share;
            };

            // a different clip still fading out is cut; the new one fades in over it
            match masked.playing {
                Some((playing, _)) if playing != clip_ix => {
                    player.stop(playing);
                    masked.playing = None;
                }
                _ => (),
            }

            let restart = request.restart || !player.is_playing_animation(clip_ix);
            let last_audio_mark = if restart {
                f32::NEG_INFINITY
            } else {
                spawned_extras
                    .get(&entity)
                    .and_then(|extras| extras.audio.as_ref())
                    .map(|(_, mark)| *mark)
                    .unwrap_or(f32::NEG_INFINITY)
            };
            let Ok(sound) = next_emote_sound(
                emote,
                &sounds,
                last_audio_mark,
                clip_duration,
                request.repeat,
            ) else {
                break 'request current_share;
            };

            if restart {
                debug!("starting masked clip {:?}", clip_ix);
                let share = masked.playing.map(|(_, share)| share).unwrap_or(0.0);
                player.start(clip_ix).set_weight(masked_weight(share));
                masked.playing = Some((clip_ix, share));
                request.restart = false;
            }

            let Some(active_animation) = player.animation_mut(clip_ix) else {
                break 'request current_share;
            };
            if request.repeat {
                active_animation.repeat();
            } else {
                active_animation.set_repeat(RepeatAnimation::Never);
            }
            active_animation.set_speed(1.0);
            if active_animation.is_finished() {
                request.finished = true;
            }
            let elapsed = active_animation.seek_time()
                + active_animation.completions() as f32 * clip_duration;

            // the prop clip runs in step with the avatar clip
            if let Some((prop_player_ents, prop_clip_ix)) = prop_player_and_clip {
                for prop_ent in prop_player_ents {
                    let Ok((mut prop_player, _)) = players.get_mut(prop_ent) else {
                        continue;
                    };
                    if restart || !prop_player.is_playing_animation(prop_clip_ix) {
                        prop_player.start(prop_clip_ix);
                    }
                    if let Some(prop_animation) = prop_player.animation_mut(prop_clip_ix) {
                        if request.repeat {
                            prop_animation.repeat();
                        } else {
                            prop_animation.set_repeat(RepeatAnimation::Never);
                        }
                    }
                }
            }

            if let Some((play_time, sound)) = sound {
                if elapsed >= play_time {
                    debug!("masked play {:?} @ {}>{}", sound.path(), elapsed, play_time);
                    let extras = spawned_extras
                        .entry(entity)
                        .or_insert_with(|| SpawnedExtras::new(request.urn.clone()));
                    emit_emote_sound(
                        &mut commands,
                        &mut emitters,
                        &mut extras.audio,
                        ent,
                        sound,
                        elapsed,
                    );
                }
            }
            1.0
        };

        let Ok((mut player, _)) = players.get_mut(ent) else {
            continue;
        };

        // drive the fade; a fully faded-out clip is stopped
        if let Some((clip_ix, share)) = masked.playing.as_mut() {
            let step = time.delta_secs() / MASKED_FADE_SECS;
            *share = if target_share > *share {
                (*share + step).min(target_share)
            } else {
                (*share - step).max(target_share)
            };
            if *share <= 0.0 {
                player.stop(*clip_ix);
                masked.playing = None;
            } else if let Some(active_animation) = player.animation_mut(*clip_ix) {
                active_animation.set_weight(masked_weight(*share));
            }
        }
    }
}

/// emote
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/emote")]
struct EmoteConsoleCommand {
    /// Emote urn, or a profile emote slot number
    urn: String,
    #[arg(long, default_value_t = false)]
    r#loop: bool,
    /// Play on the upper body only, over locomotion
    #[arg(long, default_value_t = false)]
    upper_body: bool,
}

fn emote_console_command(
    mut commands: Commands,
    mut input: ConsoleCommand<EmoteConsoleCommand>,
    player: Query<(Entity, Option<&EmoteCommand>), With<PrimaryUser>>,
    profile: Res<CurrentUserProfile>,
) {
    if let Some(Ok(command)) = input.take() {
        if let Ok((player, maybe_prev)) = player.single() {
            let mut urn = &command.urn;
            if let Ok(slot) = command.urn.parse::<u32>() {
                if let Some(emote) = profile
                    .profile
                    .as_ref()
                    .and_then(|p| p.content.avatar.emotes.as_ref())
                    .and_then(|es| es.iter().find(|e| e.slot == slot))
                {
                    urn = &emote.urn;
                }
            }

            info!("anim {} -> {}", command.urn, urn);
            let timestamp = maybe_prev.map(|p| p.timestamp + 1).unwrap_or_default();

            commands.entity(player).try_insert(EmoteCommand {
                urn: urn.clone(),
                timestamp,
                r#loop: command.r#loop,
                mask: if command.upper_body {
                    EmoteMask::UpperBody
                } else {
                    EmoteMask::FullBody
                },
            });
            input.reply_ok(format!("playing emote {urn}"));
        } else {
            input.reply_failed("player not found");
        }
    }
}

// Plays avatar-bus audio clips requested by a scene-driven movement animation.
// Dedups against the last observed sound list per avatar so that the scene holding
// the same list across frames doesn't re-fire sounds — a new play is triggered
// only when the list transitions to a different value (including the scene clearing
// and re-asserting it on a later frame).
fn play_scene_driven_sounds(
    mut commands: Commands,
    avatars: Query<(Entity, &SceneDrivenAnim)>,
    ipfas: IpfsAssetServer,
    mut last_sounds: Local<HashMap<Entity, Vec<String>>>,
) {
    let mut seen: HashSet<Entity> = HashSet::default();
    for (entity, scene_anim) in avatars.iter() {
        seen.insert(entity);
        let Some(active) = scene_anim.active.as_ref() else {
            last_sounds.remove(&entity);
            continue;
        };
        let prev = last_sounds.get(&entity);
        if prev.map(|v| v.as_slice()) == Some(active.sounds.as_slice()) {
            continue;
        }
        for content_hash in &active.sounds {
            let handle = ipfas.load_scene_content_hash::<bevy_kira_audio::AudioSource>(
                &active.scene_hash,
                content_hash,
            );
            let audio_entity = commands
                .spawn((
                    Transform::default(),
                    Visibility::default(),
                    AudioEmitter {
                        handle,
                        ty: AudioType::Avatar,
                        ..Default::default()
                    },
                ))
                .id();
            if let Ok(mut entity_commands) = commands.get_entity(entity) {
                entity_commands.try_push_children(&[audio_entity]);
            }
        }
        last_sounds.insert(entity, active.sounds.clone());
    }
    // Drop tracked state for avatars that no longer have the component.
    last_sounds.retain(|e, _| seen.contains(e));
}
