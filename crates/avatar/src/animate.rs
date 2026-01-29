use core::f32;
use std::time::Duration;

use bevy::{
    animation::RepeatAnimation,
    gltf::Gltf,
    math::Vec3Swizzles,
    platform::collections::{HashMap, HashSet},
    prelude::*,
    scene::InstanceId,
};
use bevy_console::ConsoleCommand;
use collectibles::{
    Collectible, CollectibleData, CollectibleError, CollectibleManager, Emote, EmoteUrn,
};
use common::{
    dynamics::PLAYER_COLLIDER_RADIUS,
    rpc::{RpcCall, RpcEventSender},
    sets::SceneSets,
    structs::{
        AudioEmitter, AudioType, AvatarDynamicState, EmoteCommand, MoveKind, PlayerModifiers,
        PrimaryUser,
    },
    util::TryPushChildrenEx,
};
use comms::{
    chat_marker_things,
    global_crdt::{ChatEvent, ForeignPlayer},
    profile::CurrentUserProfile,
    NetworkMessage, Transport, TransportType,
};
use console::DoAddConsoleCommand;
use dcl::interface::CrdtType;
use dcl_component::{
    proto_components::{
        kernel::comms::rfc4::{self, Chat, PlayerEmote},
        sdk::components::PbAvatarEmoteCommand,
    },
    SceneComponentId, SceneEntityId,
};
use ipfs::IpfsAssetServer;
use scene_runner::{
    permissions::Permission, renderer_context::RendererSceneContext,
    update_world::animation::Clips, ContainerEntity, ContainingScene,
};

use crate::{process_avatar, AvatarDefinition};

#[derive(Component)]
pub struct AvatarAnimPlayer(pub Entity);

pub struct AvatarAnimationPlugin;

impl Plugin for AvatarAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                (handle_trigger_emotes, broadcast_emote, receive_emotes).before(animate),
                (animate, play_current_emote).chain().after(process_avatar),
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

    for (scene, urn, r#loop) in emote_cmds.read().filter_map(|ev| {
        if let RpcCall::TriggerEmote { scene, urn, r#loop } = ev {
            Some((scene, urn, *r#loop))
        } else {
            None
        }
    }) {
        perms.check(
            common::structs::PermissionType::PlayEmote,
            *scene,
            EmoteCommand {
                urn: urn.clone(),
                r#loop,
                timestamp: maybe_prev
                    .map(|prev| prev.timestamp + 1)
                    .unwrap_or_default(),
            },
            None,
            false,
        );
    }

    for emote in perms.drain_success(common::structs::PermissionType::PlayEmote) {
        commands.entity(player).insert(emote.clone());
    }

    for _ in perms.drain_fail(common::structs::PermissionType::PlayEmote) {}
}

fn broadcast_emote(
    q: Query<&EmoteCommand, With<PrimaryUser>>,
    transports: Query<&Transport>,
    mut last: Local<Option<EmoteCommand>>,
    mut count: Local<usize>,
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

    if let Ok(emote) = q.single() {
        if last.as_ref() != Some(emote) {
            *count += 1;
            debug!("sending emote: {emote:?} {}", *count);
            let old_packet = rfc4::Packet {
                message: Some(rfc4::packet::Message::Chat(Chat {
                    message: format!("{}{} {}", chat_marker_things::EMOTE, emote.urn, *count),
                    timestamp: time.elapsed_secs_f64(),
                })),
                protocol_version: 100,
            };

            let new_packet = rfc4::Packet {
                message: Some(rfc4::packet::Message::PlayerEmote(PlayerEmote {
                    incremental_id: *count as u32,
                    urn: emote.urn.clone(),
                    timestamp: time.elapsed_secs(),
                })),
                protocol_version: 100,
            };

            for transport in transports.iter() {
                if transport.transport_type != TransportType::Archipelago {
                    if transport.transport_type != TransportType::SceneRoom {
                        let _ = transport
                            .sender
                            .blocking_send(NetworkMessage::reliable(&old_packet));
                    }

                    let _ = transport
                        .sender
                        .blocking_send(NetworkMessage::reliable(&new_packet));
                }
            }

            *last = Some(emote.clone());

            senders.retain(|sender| {
                let _ = sender.send(format!("{{ \"expressionId\": \"{}\" }}", emote.urn));
                !sender.is_closed()
            })
        }
        return;
    }

    *last = None;
}

fn receive_emotes(mut commands: Commands, mut chat_events: EventReader<ChatEvent>) {
    for ev in chat_events
        .read()
        .filter(|e| e.message.starts_with(chat_marker_things::EMOTE))
    {
        let mut emote_and_timestamp = ev
            .message
            .strip_prefix(chat_marker_things::EMOTE)
            .unwrap()
            .split(' ');
        if let (Some(emote_urn), Some(timestamp)) =
            (emote_and_timestamp.next(), emote_and_timestamp.next())
        {
            debug!("adding remote emote: {}", emote_urn);
            commands.entity(ev.sender).try_insert(EmoteCommand {
                urn: emote_urn.to_owned(),
                timestamp: timestamp.parse().unwrap_or_default(),
                r#loop: false,
            });
        }
    }
}

#[derive(Component)]
pub struct ActiveEmote {
    urn: EmoteUrn,
    speed: f32,
    restart: bool,
    repeat: bool,
    finished: bool,
    transition_seconds: f32,
    initial_audio_mark: Option<f32>,
}

impl Default for ActiveEmote {
    fn default() -> Self {
        Self {
            urn: EmoteUrn::new("idle_male").unwrap(),
            speed: 1.0,
            restart: false,
            repeat: false,
            finished: false,
            transition_seconds: 0.2,
            initial_audio_mark: None,
        }
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
        Option<&mut ActiveEmote>,
        Option<&ForeignPlayer>,
        Option<&ContainerEntity>,
        Option<&PrimaryUser>,
        Option<&mut LastEmoteCommand>,
    )>,
    mut velocities: Local<HashMap<Entity, Vec3>>,
    mut current_emote_min_velocities: Local<HashMap<Entity, f32>>,
    time: Res<Time>,
    player: Query<(&PrimaryUser, Option<&PlayerModifiers>)>,
    containing_scene: ContainingScene,
    mut scenes: Query<&mut RendererSceneContext>,
) {
    let (gravity, jump_height) = player
        .single()
        .map(|(p, m)| m.map(|m| m.combine(p)).unwrap_or(p.clone()))
        .map(|p| (p.gravity, p.jump_height))
        .unwrap_or((-20.0, 1.25));
    let gravity = gravity.min(-0.1);
    let jump_height = jump_height.max(0.1);

    let prior_velocities = std::mem::take(&mut *velocities);
    let prior_min_velocities = std::mem::take(&mut *current_emote_min_velocities);

    for (
        avatar_ent,
        mut dynamic_state,
        emote,
        gt,
        active_emote,
        maybe_foreign,
        maybe_container,
        maybe_primary,
        last_emote,
    ) in avatars.iter_mut()
    {
        let Some(mut active_emote) = active_emote else {
            commands.entity(avatar_ent).insert((
                ActiveEmote::default(),
                EmoteCommand::default(),
                LastEmoteCommand::default(),
            ));
            continue;
        };

        // calculate/store damped velocity
        let prior_velocity = prior_velocities
            .get(&avatar_ent)
            .copied()
            .unwrap_or(Vec3::ZERO);
        let ratio = time.delta_secs().clamp(0.0, 0.1) / 0.1;
        let damped_velocity =
            dynamic_state.force.extend(0.0).xzy() * ratio + prior_velocity * (1.0 - ratio);
        let damped_velocity_len = damped_velocity.xz().length();
        velocities.insert(avatar_ent, damped_velocity);

        // get requested emote
        let (mut requested_emote, given_urn, request_loop) =
            if let Some(EmoteCommand { urn, r#loop, .. }) = emote {
                (EmoteUrn::new(urn.as_str()).ok(), Some(urn), *r#loop)
            } else {
                (None, None, false)
            };

        let emote_changed = emote != last_emote.as_ref().map(|l| &l.0);

        // check expired
        if !emote_changed && Some(&active_emote.urn) != requested_emote.as_ref() {
            requested_emote = None;
        }

        // check / cancel requested emote
        if Some(&active_emote.urn) == requested_emote.as_ref() {
            let playing_min_vel = prior_min_velocities
                .get(&avatar_ent)
                .copied()
                .unwrap_or_default();
            if damped_velocity_len * 0.9 > playing_min_vel {
                // stop emotes on move
                debug!(
                    "clear on motion {} > {}",
                    damped_velocity_len, playing_min_vel
                );
                requested_emote = None;
            } else {
                current_emote_min_velocities
                    .insert(avatar_ent, damped_velocity_len.min(playing_min_vel));
            }

            if active_emote.finished {
                debug!("finished emoting {:?}", active_emote.urn);
                requested_emote = None;
            }
        } else {
            current_emote_min_velocities.insert(avatar_ent, damped_velocity_len);
        }

        // play requested emote
        *active_emote = if let Some(requested_emote) = requested_emote {
            if emote_changed {
                dynamic_state.move_kind = MoveKind::Emote;

                // send to scenes
                let broadcast_urn = given_urn.unwrap();
                debug!("broadcasting emote to scenes: {:?}", broadcast_urn);

                let (scene, scene_id) = match (maybe_foreign, maybe_primary, maybe_container) {
                    (Some(f), ..) => (None, f.scene_id),
                    (None, Some(_), _) => (None, SceneEntityId::PLAYER),
                    (None, None, Some(container)) => {
                        (Some(container.container), container.container_id)
                    }
                    _ => (Some(Entity::PLACEHOLDER), SceneEntityId::ROOT),
                };

                let report_scenes = match scene {
                    Some(scene) => vec![scene],
                    None => containing_scene
                        .get_area(avatar_ent, PLAYER_COLLIDER_RADIUS)
                        .into_iter()
                        .collect(),
                };

                for scene_ent in report_scenes {
                    let Ok(mut scene) = scenes.get_mut(scene_ent) else {
                        warn!("no scene to receive emote");
                        continue;
                    };

                    let timestamp = scene.tick_number;
                    debug!("broadcast to scene {:?}", scene_ent);
                    scene.update_crdt(
                        SceneComponentId::AVATAR_EMOTE_COMMAND,
                        CrdtType::GO_ANY,
                        scene_id,
                        &PbAvatarEmoteCommand {
                            emote_urn: broadcast_urn.to_string(),
                            r#loop: request_loop,
                            timestamp,
                        },
                    );
                }
            }
            commands
                .entity(avatar_ent)
                .insert(LastEmoteCommand(emote.unwrap().clone()));
            ActiveEmote {
                urn: requested_emote,
                restart: emote_changed,
                repeat: request_loop,
                ..Default::default()
            }
        } else {
            // otherwise play a default emote based on motion
            let time_to_peak = (jump_height * -gravity * 2.0).sqrt() / -gravity;
            let just_jumped =
                dynamic_state.jump_time > (time.elapsed_secs() - time_to_peak / 2.0).max(0.0);
            if dynamic_state.ground_height > 0.2 || (dynamic_state.velocity.y > 0.0 && just_jumped)
            {
                if just_jumped {
                    dynamic_state.move_kind = MoveKind::Jump;
                } else {
                    dynamic_state.move_kind = MoveKind::Falling;
                }
                ActiveEmote {
                    urn: EmoteUrn::new("jump").unwrap(),
                    speed: time_to_peak.recip() * 0.75,
                    repeat: true,
                    restart: dynamic_state.jump_time > time.elapsed_secs() - time.delta_secs(),
                    transition_seconds: 0.1,
                    initial_audio_mark: if !just_jumped { Some(0.1) } else { None },
                    ..Default::default()
                }
            } else if active_emote.urn == EmoteUrn::new("jump").unwrap() && !active_emote.finished {
                // finish the jump - we use `repeat: false` to signal that we are landing...
                ActiveEmote {
                    urn: EmoteUrn::new("jump").unwrap(),
                    speed: 1.5,
                    repeat: false,
                    restart: false,
                    transition_seconds: 0.1,
                    initial_audio_mark: Some(0.1),
                    ..Default::default()
                }
            } else {
                let directional_velocity_len =
                    (damped_velocity * (Vec3::X + Vec3::Z)).dot(gt.forward().as_vec3());

                if damped_velocity_len.abs() > 0.1 {
                    if damped_velocity_len.abs() <= 2.6 {
                        dynamic_state.move_kind = MoveKind::Walk;
                        ActiveEmote {
                            urn: EmoteUrn::new("walk").unwrap(),
                            speed: directional_velocity_len / 1.5,
                            restart: false,
                            repeat: true,
                            ..Default::default()
                        }
                    } else {
                        dynamic_state.move_kind = MoveKind::Jog;
                        ActiveEmote {
                            urn: EmoteUrn::new("run").unwrap(),
                            speed: directional_velocity_len / 4.5,
                            restart: false,
                            repeat: true,
                            ..Default::default()
                        }
                    }
                } else {
                    dynamic_state.move_kind = MoveKind::Idle;
                    ActiveEmote {
                        urn: EmoteUrn::new("idle_male").unwrap(),
                        speed: 1.0,
                        restart: false,
                        repeat: true,
                        ..Default::default()
                    }
                }
            }
        }
    }
}

struct SpawnedExtras {
    urn: EmoteUrn,
    scene: Option<InstanceId>,
    scene_initialized: bool,
    audio: Option<(Entity, f32)>,
    clip: Option<(AnimationNodeIndex, Handle<AnimationGraph>)>,
}

impl SpawnedExtras {
    pub fn new(urn: EmoteUrn) -> Self {
        Self {
            urn,
            scene: None,
            scene_initialized: false,
            audio: None,
            clip: None,
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn play_current_emote(
    mut commands: Commands,
    mut q: Query<(Entity, &mut ActiveEmote, &AvatarAnimPlayer, &Children)>,
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
    mut spawned_extras: Local<HashMap<Entity, SpawnedExtras>>,
    mut scene_spawner: ResMut<SceneSpawner>,
    (sounds, anim_clips): (
        Res<Assets<bevy_kira_audio::AudioSource>>,
        Res<Assets<AnimationClip>>,
    ),
    mut emitters: Query<&mut AudioEmitter>,
    prop_details: Query<(Option<&Name>, &Transform, &ChildOf)>,
) {
    let prior_playing = std::mem::take(&mut *playing);
    let mut prev_spawned_extras = std::mem::take(&mut *spawned_extras);

    for (entity, mut active_emote, target_entity, children) in q.iter_mut() {
        debug!("emote {}", active_emote.urn);
        let Some(definition) = children.iter().flat_map(|c| definitions.get(c).ok()).next() else {
            warn!("no definition");
            continue;
        };

        // clean up old extras
        if let Some(extras) = prev_spawned_extras.remove(&entity) {
            if extras.urn != active_emote.urn {
                if let Some(scene) = extras.scene {
                    scene_spawner.despawn_instance(scene);
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

        if let Some(scene_emote) = active_emote.urn.scene_emote() {
            debug!("got {scene_emote:?}");
            let mut split = scene_emote.split('-');
            let maybe_hash = if split.next() == Some("b64") {
                // stupid to have "-" as a separator and also part of the hash itself for local hashes
                let parts = split.skip(1).take(2).collect::<Vec<_>>();
                (parts.len() == 2).then_some(parts.join("-"))
            } else {
                split.next().map(ToOwned::to_owned)
            };
            let Some(hash) = maybe_hash.as_ref() else {
                debug!("failed to split scene emote {scene_emote:?}");
                active_emote.finished = true;
                continue;
            };

            if emote_loader
                .get_representation(&active_emote.urn, bodyshape.as_str())
                .is_err()
            {
                // load the gltf
                let handle = ipfas.load_hash::<Gltf>(hash);
                let gltf = match gltfs.get_mut(handle.id()) {
                    Some(gltf) => {
                        cached_gltf_handles.remove(&handle);
                        gltf
                    }
                    None => {
                        cached_gltf_handles.insert(handle);
                        continue;
                    }
                };

                // fix up the gltf if possible/required
                if !gltf.named_animations.keys().any(|k| k.ends_with("_Avatar")) {
                    let Some(anim) = gltf.animations.first() else {
                        warn!("scene emote has no animations");
                        active_emote.finished = true;
                        continue;
                    };

                    gltf.named_animations.insert("_Avatar".into(), anim.clone());
                }

                // add repr
                emote_loader.add_builtin(
                    active_emote.urn.clone(),
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
                            urn: active_emote.urn.as_str().to_owned(),
                            thumbnail: "embedded://images/redx.png".to_owned(),
                            available_representations: HashSet::from_iter([bodyshape.to_owned()]),
                            name: active_emote.urn.to_string(),
                            description: active_emote.urn.to_string(),
                            extra_data: (),
                        },
                    },
                );
            }
        }

        let emote = match emote_loader.get_representation(&active_emote.urn, bodyshape.as_str()) {
            Ok(emote) => emote,
            e @ Err(CollectibleError::Failed)
            | e @ Err(CollectibleError::Missing)
            | e @ Err(CollectibleError::NoRepresentation) => {
                debug!("{} -> {:?}", active_emote.urn, e);
                active_emote.finished = true;
                continue;
            }
            Err(CollectibleError::Loading) => {
                debug!("{} -> loading", active_emote.urn);
                continue;
            }
        };
        active_emote.repeat |= emote.default_repeat;

        let clip = match emote.avatar_animation(&gltfs) {
            Err(e) => {
                debug!("animation error: {:?}", e);
                continue;
            }
            Ok(None) => {
                debug!("{} -> no clip", active_emote.urn);
                debug!(
                    "available : {:?}",
                    gltfs
                        .get(emote.gltf.id())
                        .map(|gltf| gltf.named_animations.keys().collect::<Vec<_>>())
                );
                active_emote.finished = true;
                continue;
            }
            Ok(Some(clip)) => clip,
        };

        // extract props and prop anim
        let mut prop_player_and_clip = None;
        if let Ok(Some(props)) = emote.prop_scene(&gltfs) {
            debug!("got props");
            if let Some(extras) = spawned_extras.get_mut(&entity) {
                let Some(instance) = extras.scene else {
                    continue;
                };

                if !scene_spawner.instance_is_ready(instance) {
                    continue;
                }

                if !extras.scene_initialized {
                    for spawned_ent in scene_spawner.iter_instance_entities(instance) {
                        if let Ok((maybe_name, transform, parent)) = prop_details.get(spawned_ent) {
                            // hide stuff like unity
                            // what a mess
                            if let Some(name) =
                                maybe_name.map(Name::as_str).map(str::to_ascii_lowercase)
                            {
                                if name.contains("_reference")
                                    || name.ends_with("_basemesh")
                                    || name.starts_with("m_mask_")
                                {
                                    commands.entity(spawned_ent).try_insert(Visibility::Hidden);
                                    warn!("hiding emote prop `{name}` due to name");
                                }
                            }

                            if parent.parent() == entity {
                                // children of root nodes -> rotate
                                if parent.parent() == entity {
                                    let mut rotated = *transform;
                                    rotated.rotate_around(
                                        Vec3::ZERO,
                                        Quat::from_rotation_y(std::f32::consts::PI),
                                    );
                                    commands.entity(spawned_ent).try_insert(rotated);
                                }
                            }
                        }
                    }
                    extras.scene_initialized = true;
                }

                if let Ok(Some(prop_clip)) = emote.prop_anim(&gltfs) {
                    let clip = extras.clip.get_or_insert_with(|| {
                        let (graph, ix) = AnimationGraph::from_clip(prop_clip);
                        (ix, graphs.add(graph))
                    });

                    let prop_players = scene_spawner
                        .iter_instance_entities(instance)
                        .filter(|ent| {
                            if let Ok((_, _, _, g)) = players.get(*ent) {
                                if g.is_none() {
                                    commands
                                        .entity(*ent)
                                        .insert(AnimationGraphHandle(clip.1.clone()));
                                }
                                true
                            } else {
                                false
                            }
                        })
                        .collect::<Vec<_>>();
                    if !prop_players.is_empty() {
                        prop_player_and_clip = Some((prop_players, clip.0))
                    }
                }
            } else {
                let scene = scene_spawner.spawn_as_child(props, entity);
                spawned_extras
                    .entry(entity)
                    .or_insert_with(|| SpawnedExtras::new(active_emote.urn.clone()))
                    .scene = Some(scene);
                continue;
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
        let completions = (last_audio_mark / clip_duration).floor();

        // get next time to play a sound, with a lot of messing around for inf values
        let sound = match emote.audio(
            &sounds,
            if last_audio_mark.is_finite() {
                last_audio_mark % clip_duration
            } else {
                last_audio_mark
            },
        ) {
            Ok(Some((t, s))) => {
                if last_audio_mark.is_finite() {
                    Some((t + completions * clip_duration, s))
                } else {
                    Some((t, s))
                }
            }
            Ok(None) => None,
            Err(_) => continue,
        };
        debug!(
            "audio with mark {last_audio_mark} -> {:?}",
            sound.as_ref().map(|(t, _)| t)
        );
        let sound = if sound.is_none() && active_emote.repeat {
            match emote.audio(&sounds, f32::NEG_INFINITY) {
                Ok(None) => None,
                Ok(Some((play_time, s))) => {
                    Some((play_time + clip_duration * (completions + 1.0), s))
                }
                Err(_) => continue,
            }
        } else {
            sound
        };

        let play = |transitions: Option<Mut<AnimationTransitions>>,
                    player: &mut AnimationPlayer,
                    clip_ix: AnimationNodeIndex,
                    active_emote: &ActiveEmote|
         -> f32 {
            let active_animation =
                if Some(&active_emote.urn) != prior_playing.get(&ent) || active_emote.restart {
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

                // nasty hack for falling animation
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
        let (clip_ix, _) = clips
            .named
            .entry(active_emote.urn.to_string())
            .or_insert_with(|| {
                debug!("adding clip");
                let Some(graph) = graph.and_then(|graph| graphs.get_mut(graph)) else {
                    return (AnimationNodeIndex::new(u32::MAX as usize), 0.0);
                };
                (graph.add_clip(clip, 1.0, graph.root), 0.0)
            });

        let elapsed = play(transitions, &mut player, *clip_ix, &active_emote);
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
                    play(transitions, &mut player, clip_ix, &active_emote);
                }
            }
        }

        active_emote.restart = false;

        if let Some((play_time, sound)) = sound {
            if elapsed >= play_time {
                debug!("duration {}", clip_duration);
                debug!("play {:?} @ {}>{}", sound.path(), elapsed, play_time);
                let existing = spawned_extras
                    .get_mut(&entity)
                    .and_then(|extras| extras.audio.as_mut());
                if let Some(mut existing_emitter) = existing
                    .as_ref()
                    .and_then(|(e, _)| emitters.get_mut(*e).ok())
                {
                    *existing_emitter = AudioEmitter {
                        handle: sound,
                        ty: AudioType::Avatar,
                        ..Default::default()
                    };
                    existing.unwrap().1 = elapsed;
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

                    if let Ok(mut commands) = commands.get_entity(ent) {
                        commands.try_push_children(&[audio_entity]);
                    }

                    spawned_extras
                        .entry(entity)
                        .or_insert_with(|| SpawnedExtras::new(active_emote.urn.clone()))
                        .audio = Some((audio_entity, elapsed));
                }
            }
        }

        playing.insert(ent, active_emote.urn.clone());
    }
}

/// emote
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/emote")]
struct EmoteConsoleCommand {
    urn: String,
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
            let timestamp = maybe_prev.map(|p| p.timestamp).unwrap_or_default();

            commands.entity(player).try_insert(EmoteCommand {
                urn: urn.clone(),
                timestamp,
                r#loop: false,
            });
        };
        input.ok();
    }
}
