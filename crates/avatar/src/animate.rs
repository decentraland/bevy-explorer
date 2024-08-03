use std::{collections::VecDeque, time::Duration};

use bevy::{
    animation::RepeatAnimation,
    gltf::Gltf,
    math::Vec3Swizzles,
    prelude::*,
    scene::InstanceId,
    utils::{HashMap, HashSet},
};
use bevy_console::ConsoleCommand;
use bevy_kira_audio::AudioControl;
use collectibles::{
    Collectible, CollectibleData, CollectibleError, CollectibleManager, Emote, EmoteUrn,
};
use common::{
    dynamics::PLAYER_COLLIDER_RADIUS,
    rpc::{RpcCall, RpcEventSender},
    sets::SceneSets,
    structs::PrimaryUser,
};
use comms::{
    chat_marker_things,
    global_crdt::{ChatEvent, ForeignPlayer},
    profile::CurrentUserProfile,
    NetworkMessage, Transport,
};
use console::DoAddConsoleCommand;
use dcl::interface::{ComponentPosition, CrdtType};
use dcl_component::{
    proto_components::{
        kernel::comms::rfc4::{self, Chat},
        sdk::components::PbAvatarEmoteCommand,
    },
    SceneComponentId, SceneEntityId,
};
use ipfs::IpfsAssetServer;
use scene_runner::{
    permissions::Permission,
    renderer_context::RendererSceneContext,
    update_world::{
        avatar_modifier_area::PlayerModifiers,
        transform_and_parent::{ParentPositionSync, SceneProxyStage},
        AddCrdtInterfaceExt,
    },
    ContainerEntity, ContainingScene,
};

use crate::{process_avatar, AvatarDefinition};

use super::AvatarDynamicState;

#[derive(Component)]
pub struct AvatarAnimPlayer(pub Entity);

pub struct AvatarAnimationPlugin;

#[derive(Debug, Clone)]
pub struct EmoteCommand {
    pub emote: PbAvatarEmoteCommand,
    pub broadcast: EmoteBroadcast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmoteBroadcast {
    All,
    None,
    Omit(Entity),
}

#[derive(Component, Default, Deref, DerefMut, Debug, Clone)]
pub struct EmotesFromScene(pub(crate) VecDeque<PbAvatarEmoteCommand>);

#[derive(Component, Default, Deref, DerefMut, Debug, Clone)]
pub struct EmoteList(pub(crate) VecDeque<EmoteCommand>);

impl EmoteList {
    pub fn new(emote_urn: impl Into<String>, origin: EmoteBroadcast) -> Self {
        Self(VecDeque::from_iter([EmoteCommand {
            emote: PbAvatarEmoteCommand {
                emote_urn: emote_urn.into(),
                r#loop: false,
                timestamp: 0,
            },
            broadcast: origin,
        }]))
    }
}

impl Plugin for AvatarAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_go_component::<PbAvatarEmoteCommand, EmotesFromScene>(
            SceneComponentId::AVATAR_EMOTE_COMMAND,
            ComponentPosition::EntityOnly,
        );
        app.add_systems(
            Update,
            (
                (read_player_emotes, broadcast_emote, receive_emotes).before(animate),
                (animate, play_current_emote).chain().after(process_avatar),
            )
                .in_set(SceneSets::PostLoop),
        );
        app.add_console_command::<EmoteConsoleCommand, _>(emote_console_command);
    }
}

// copy emotes from scene-player entities onto main player entity
#[allow(clippy::type_complexity)]
fn read_player_emotes(
    mut commands: Commands,
    scene_player_emotes: Query<
        (
            Entity,
            Ref<EmotesFromScene>,
            &ParentPositionSync<SceneProxyStage>,
            &ContainerEntity,
        ),
        Without<PrimaryUser>,
    >,
    player: Query<Entity, With<PrimaryUser>>,
    mut perms: Permission<EmoteList>,
) {
    let Ok(player) = player.get_single() else {
        return;
    };

    for (scene_ent, emotes, parent, container) in &scene_player_emotes {
        if emotes.0.is_empty() || !emotes.is_changed() {
            continue;
        }

        let mut list = EmoteList::default();
        for emote in &emotes.0 {
            list.0.push_back(EmoteCommand {
                emote: emote.to_owned(),
                broadcast: EmoteBroadcast::Omit(container.root),
            })
        }

        if parent.0 == player {
            commands.entity(scene_ent).remove::<EmoteList>();
            perms.check(
                common::structs::PermissionType::PlayEmote,
                container.root,
                list,
                None,
                false,
            );
        } else {
            commands.entity(player).insert(list);
        }
    }

    for list in perms.drain_success(common::structs::PermissionType::PlayEmote) {
        commands.entity(player).insert(list.clone());
    }

    for _ in perms.drain_fail(common::structs::PermissionType::PlayEmote) {}
}

fn broadcast_emote(
    q: Query<&EmoteList, With<PrimaryUser>>,
    transports: Query<&Transport>,
    mut last: Local<Option<String>>,
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

    for list in q.iter() {
        if let Some(EmoteCommand {
            emote: PbAvatarEmoteCommand { emote_urn, .. },
            ..
        }) = list.back()
        {
            if last.as_ref() != Some(emote_urn) {
                *count += 1;
                debug!("sending emote: {emote_urn:?} {}", *count);
                let packet = rfc4::Packet {
                    message: Some(rfc4::packet::Message::Chat(Chat {
                        message: format!("{}{} {}", chat_marker_things::EMOTE, emote_urn, *count),
                        timestamp: time.elapsed_seconds_f64(),
                    })),
                    protocol_version: 999,
                };

                for transport in transports.iter() {
                    let _ = transport
                        .sender
                        .blocking_send(NetworkMessage::reliable(&packet));
                }

                *last = Some(emote_urn.to_owned());

                senders.retain(|sender| {
                    let _ = sender.send(format!("{{ \"expressionId\": \"{emote_urn}\" }}"));
                    !sender.is_closed()
                })
            }
            return;
        }

        *last = None;
    }
}

fn receive_emotes(mut commands: Commands, mut chat_events: EventReader<ChatEvent>) {
    for ev in chat_events
        .read()
        .filter(|e| e.message.starts_with(chat_marker_things::EMOTE))
    {
        if let Some(emote_urn) = ev
            .message
            .strip_prefix(chat_marker_things::EMOTE)
            .unwrap()
            .split(' ')
            .next()
        {
            debug!("adding remote emote: {}", emote_urn);
            commands
                .entity(ev.sender)
                .try_insert(EmoteList::new(emote_urn, EmoteBroadcast::All));
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
        &AvatarDynamicState,
        Option<&mut EmoteList>,
        &GlobalTransform,
        Option<&mut ActiveEmote>,
        Option<&ForeignPlayer>,
    )>,
    mut velocities: Local<HashMap<Entity, Vec3>>,
    mut current_emote_min_velocities: Local<HashMap<Entity, f32>>,
    time: Res<Time>,
    player: Query<(&PrimaryUser, Option<&PlayerModifiers>)>,
    containing_scene: ContainingScene,
    mut scenes: Query<&mut RendererSceneContext>,
) {
    let (gravity, jump_height) = player
        .get_single()
        .map(|(p, m)| m.map(|m| m.combine(p)).unwrap_or(p.clone()))
        .map(|p| (p.gravity, p.jump_height))
        .unwrap_or((-20.0, 1.25));
    let gravity = gravity.min(-0.1);
    let jump_height = jump_height.max(0.1);

    let prior_velocities = std::mem::take(&mut *velocities);
    let prior_min_velocities = std::mem::take(&mut *current_emote_min_velocities);

    for (avatar_ent, dynamic_state, mut emotes, gt, active_emote, maybe_foreign) in
        avatars.iter_mut()
    {
        let Some(mut active_emote) = active_emote else {
            commands.entity(avatar_ent).insert(ActiveEmote::default());
            continue;
        };

        // take a copy of the last entry, remove others
        let emotes_changed = emotes.as_ref().map_or(false, Mut::is_changed);
        let emote = emotes
            .as_mut()
            .map(|e| {
                let last = e.pop_back();
                e.clear();
                if let Some(last) = last.clone() {
                    e.push_back(last);
                }
                last
            })
            .unwrap_or_default();

        // calculate/store damped velocity
        let prior_velocity = prior_velocities
            .get(&avatar_ent)
            .copied()
            .unwrap_or(Vec3::ZERO);
        let ratio = time.delta_seconds().clamp(0.0, 0.1) / 0.1;
        let damped_velocity =
            dynamic_state.force.extend(0.0).xzy() * ratio + prior_velocity * (1.0 - ratio);
        let damped_velocity_len = damped_velocity.xz().length();
        velocities.insert(avatar_ent, damped_velocity);

        // get requested emote
        let (mut requested_emote, given_urn, request_loop, origin) = if let Some(EmoteCommand {
            emote: PbAvatarEmoteCommand {
                emote_urn, r#loop, ..
            },
            broadcast: origin,
        }) = emote
        {
            (
                EmoteUrn::new(emote_urn.as_str()).ok(),
                Some(emote_urn),
                r#loop,
                origin,
            )
        } else {
            (None, None, false, EmoteBroadcast::None)
        };

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
                if let Some(emotes) = emotes.as_mut() {
                    emotes.clear();
                }
                requested_emote = None;
            } else {
                current_emote_min_velocities
                    .insert(avatar_ent, damped_velocity_len.min(playing_min_vel));
            }

            if active_emote.finished && !emotes_changed {
                debug!("finished emoting {:?}", active_emote.urn);
                requested_emote = None;

                if let Some(emotes) = emotes.as_mut() {
                    emotes.clear();
                }
            }
        } else {
            current_emote_min_velocities.insert(avatar_ent, damped_velocity_len);
        }

        // play requested emote
        *active_emote = if let Some(requested_emote) = requested_emote {
            if emotes_changed && origin != EmoteBroadcast::None {
                // send to scenes
                let broadcast_urn = given_urn.unwrap();
                debug!("broadcasting emote to scenes: {:?}", broadcast_urn);

                let scene_id = maybe_foreign
                    .map(|f| f.scene_id)
                    .unwrap_or(SceneEntityId::PLAYER);
                for scene_ent in containing_scene.get_area(avatar_ent, PLAYER_COLLIDER_RADIUS) {
                    if EmoteBroadcast::Omit(scene_ent) == origin {
                        // skip the scene that created the emote
                        continue;
                    }

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
            ActiveEmote {
                urn: requested_emote,
                restart: emotes_changed,
                repeat: request_loop,
                ..Default::default()
            }
        } else {
            // otherwise play a default emote baesd on motion
            let time_to_peak = (jump_height * -gravity * 2.0).sqrt() / -gravity;
            if dynamic_state.ground_height > 0.2
                || dynamic_state.velocity.y > 0.0
                    && dynamic_state.jump_time
                        > (time.elapsed_seconds() - time_to_peak / 2.0).max(0.0)
            {
                ActiveEmote {
                    urn: EmoteUrn::new("jump").unwrap(),
                    speed: time_to_peak.recip() * 0.75,
                    repeat: true,
                    transition_seconds: 0.1,
                    ..Default::default()
                }
            } else {
                let directional_velocity_len =
                    (damped_velocity * (Vec3::X + Vec3::Z)).dot(gt.forward());

                if damped_velocity_len.abs() > 0.1 {
                    if damped_velocity_len.abs() <= 2.5 {
                        ActiveEmote {
                            urn: EmoteUrn::new("walk").unwrap(),
                            speed: directional_velocity_len / 1.5,
                            restart: false,
                            repeat: true,
                            ..Default::default()
                        }
                    } else {
                        ActiveEmote {
                            urn: EmoteUrn::new("run").unwrap(),
                            speed: directional_velocity_len / 4.5,
                            restart: false,
                            repeat: true,
                            ..Default::default()
                        }
                    }
                } else {
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
    scene_rotated: bool,
    audio: Option<Entity>,
}

impl SpawnedExtras {
    pub fn new(urn: EmoteUrn) -> Self {
        Self {
            urn,
            scene: None,
            scene_rotated: false,
            audio: None,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn play_current_emote(
    mut commands: Commands,
    mut q: Query<(Entity, &mut ActiveEmote, &AvatarAnimPlayer, &Children)>,
    definitions: Query<&AvatarDefinition>,
    mut emote_loader: CollectibleManager<Emote>,
    mut gltfs: ResMut<Assets<Gltf>>,
    animations: Res<Assets<AnimationClip>>,
    mut players: Query<(&mut AnimationPlayer, &Name)>,
    mut playing: Local<HashMap<Entity, EmoteUrn>>,
    ipfas: IpfsAssetServer,
    mut cached_gltf_handles: Local<HashSet<Handle<Gltf>>>,
    mut spawned_extras: Local<HashMap<Entity, SpawnedExtras>>,
    mut scene_spawner: ResMut<SceneSpawner>,
    audio: Res<bevy_kira_audio::Audio>,
    sounds: Res<Assets<bevy_kira_audio::AudioSource>>,
    transform_and_parent: Query<(&Transform, &Parent)>,
) {
    let prior_playing = std::mem::take(&mut *playing);
    let mut prev_spawned_extras = std::mem::take(&mut *spawned_extras);

    for (entity, mut active_emote, target_entity, children) in q.iter_mut() {
        let Some(definition) = children
            .iter()
            .flat_map(|c| definitions.get(*c).ok())
            .next()
        else {
            warn!("no definition");
            continue;
        };

        // clean up old extras
        if let Some(extras) = prev_spawned_extras.remove(&entity) {
            if extras.urn != active_emote.urn {
                if let Some(scene) = extras.scene {
                    scene_spawner.despawn_instance(scene);
                }

                if let Some(audio_ent) = extras.audio {
                    if let Some(commands) = commands.get_entity(audio_ent) {
                        commands.despawn_recursive();
                    }
                }
            } else {
                spawned_extras.insert(entity, extras);
            }
        }

        let ent = target_entity.0;
        let bodyshape = &definition.body_shape;

        if let Some(scene_emote) = active_emote.urn.scene_emote() {
            let Some((hash, _)) = scene_emote.split_once('-') else {
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
                let Some(gltf) = gltfs.get_mut(handle.id()) else {
                    if !cached_gltf_handles.contains(&handle) {
                        cached_gltf_handles.insert(handle);
                    }
                    continue;
                };

                // fix up the gltf if possible/required
                if !gltf.named_animations.keys().any(|k| k.ends_with("_Avatar")) {
                    let Some(anim) = gltf.animations.first() else {
                        warn!("scene emote has no animations");
                        active_emote.finished = true;
                        continue;
                    };

                    gltf.named_animations
                        .insert("_Avatar".to_owned(), anim.clone());
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
                                sound: None,
                            },
                        )]),
                        data: CollectibleData::<Emote> {
                            hash: hash.to_owned(),
                            urn: active_emote.urn.as_str().to_owned(),
                            thumbnail: ipfas.asset_server().load("images/redx.png"),
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

        let clip = match emote.avatar_animation(&gltfs) {
            Err(_) => continue,
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
            if let Some(instance) = spawned_extras
                .get(&entity)
                .and_then(|extras| extras.scene.as_ref())
                .copied()
            {
                if !scene_spawner.instance_is_ready(instance) {
                    continue;
                }

                let scene_rotated = spawned_extras
                    .get_mut(&entity)
                    .map(|extras| &mut extras.scene_rotated)
                    .unwrap();
                if !*scene_rotated {
                    for spawned_ent in scene_spawner.iter_instance_entities(instance) {
                        if let Ok((transform, parent)) = transform_and_parent.get(spawned_ent) {
                            if parent.get() == entity {
                                // children of root nodes -> rotate
                                if parent.get() == entity {
                                    let mut rotated = *transform;
                                    rotated.rotate_around(
                                        Vec3::ZERO,
                                        Quat::from_rotation_y(std::f32::consts::PI),
                                    );
                                    commands.entity(spawned_ent).try_insert(rotated);
                                }
                            }
                        }

                        if let Some(layers) = definition.render_layer {
                            commands.entity(spawned_ent).insert(layers);
                        }
                    }
                    *scene_rotated = true;
                }

                if let Ok(Some(prop_clip)) = emote.prop_anim(&gltfs) {
                    let Some(clip) = animations.get(prop_clip.id()) else {
                        continue;
                    };

                    if let Some(prop_player) =
                        scene_spawner.iter_instance_entities(instance).find(|ent| {
                            players
                                .get(*ent)
                                .map_or(false, |(_, name)| clip.compatible_with(name))
                        })
                    {
                        prop_player_and_clip = Some((prop_player, prop_clip));
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

        let sound = match emote.audio(&sounds) {
            Ok(s) => s,
            Err(_) => continue,
        };

        if let Some(sound) = sound {
            if spawned_extras
                .get(&entity)
                .and_then(|extras| extras.audio.as_ref())
                .is_none()
            {
                let mut handle = audio.play(sound);

                let audio_entity = commands
                    .spawn((
                        SpatialBundle::default(),
                        bevy_kira_audio::prelude::AudioEmitter {
                            instances: vec![handle.handle()],
                        },
                    ))
                    .id();

                spawned_extras
                    .entry(entity)
                    .or_insert_with(|| SpawnedExtras::new(active_emote.urn.clone()))
                    .audio = Some(audio_entity);
            }
        }

        let play = |player: &mut AnimationPlayer,
                    clip: Handle<AnimationClip>,
                    active_emote: &ActiveEmote| {
            if Some(&active_emote.urn) != prior_playing.get(&ent) || active_emote.restart {
                player.play_with_transition(
                    clip.clone(),
                    Duration::from_secs_f32(active_emote.transition_seconds),
                );
                player.seek_to(0.0);
                if active_emote.repeat {
                    player.repeat();
                } else {
                    player.set_repeat(RepeatAnimation::Never);
                }
            }

            // nasty hack for falling animation
            if active_emote.urn.as_str() == "urn:decentraland:off-chain:base-emotes:jump"
                && player.seek_time() >= 0.5833
            {
                player.seek_to(0.5833);
                player.pause();
            } else {
                player.resume();
            }

            player.set_speed(active_emote.speed);
            // on my version of bevy animator this means "should go back to starting position when finished"
            player.set_should_reset(false);
        };

        let Ok((mut player, _)) = players.get_mut(ent) else {
            debug!("no player");
            active_emote.finished = true;
            continue;
        };
        play(&mut player, clip.clone(), &active_emote);
        active_emote.restart = false;

        if !active_emote.finished && player.is_finished() {
            // debug!("finished on seek time: {}", player.seek_time());
            // we have to mess around to allow transitions to still apply even though the animation is finished.
            // assuming a new animation is `play_with_transition`ed next frame, the speed and seek position
            // here will only apply to the outgoing animation, and will allow it to be transitioned out smoothly.
            // otherwise if we let it run to actual completion then it applies no weight when it is the outgoing transition.
            let seek_time = player.seek_time() - 0.0001;
            player.seek_to(seek_time);
            player.set_speed(0.0);
            active_emote.finished = true;
        }

        if let Some((prop_player_ent, clip)) = prop_player_and_clip {
            if let Ok((mut player, _)) = players.get_mut(prop_player_ent) {
                play(&mut player, clip, &active_emote);
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
    player: Query<Entity, With<PrimaryUser>>,
    profile: Res<CurrentUserProfile>,
) {
    if let Some(Ok(command)) = input.take() {
        if let Ok(player) = player.get_single() {
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

            commands
                .entity(player)
                .try_insert(EmoteList::new(urn.clone(), EmoteBroadcast::All));
        };
        input.ok();
    }
}
