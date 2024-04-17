use std::{collections::VecDeque, time::Duration};

use bevy::{
    animation::RepeatAnimation, gltf::Gltf, math::Vec3Swizzles, prelude::*, utils::HashMap,
};
use bevy_console::ConsoleCommand;
use collectibles::{
    emotes::base_bodyshapes, CollectibleError, CollectibleManager, Emote, EmoteUrn,
};
use common::{
    rpc::{RpcCall, RpcEventSender},
    sets::SceneSets,
    structs::PrimaryUser,
};
use comms::{
    chat_marker_things,
    global_crdt::ChatEvent,
    profile::{CurrentUserProfile, UserProfile},
    NetworkMessage, Transport,
};
use console::DoAddConsoleCommand;
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::{
        kernel::comms::rfc4::{self, Chat},
        sdk::components::{pb_avatar_emote_command::EmoteCommand, PbAvatarEmoteCommand},
    },
    SceneComponentId,
};
use scene_runner::{
    update_world::{transform_and_parent::ParentPositionSync, AddCrdtInterfaceExt},
    ContainerEntity, ContainingScene,
};

use crate::process_avatar;

use super::AvatarDynamicState;

#[derive(Component)]
pub struct AvatarAnimPlayer(pub Entity);

pub struct AvatarAnimationPlugin;

#[derive(Component, Default, Deref, DerefMut, Debug, Clone)]
pub struct EmoteList(VecDeque<PbAvatarEmoteCommand>);

impl EmoteList {
    pub fn new(emote_urn: impl Into<String>) -> Self {
        Self(VecDeque::from_iter([PbAvatarEmoteCommand {
            emote_command: Some(EmoteCommand {
                emote_urn: emote_urn.into(),
                r#loop: false,
            }),
        }]))
    }
}

impl Plugin for AvatarAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_go_component::<PbAvatarEmoteCommand, EmoteList>(
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
fn read_player_emotes(
    mut commands: Commands,
    scene_player_emotes: Query<
        (Entity, &EmoteList, &ParentPositionSync, &ContainerEntity),
        Without<PrimaryUser>,
    >,
    mut player_emotes: Query<Entity, With<PrimaryUser>>,
    containing_scene: ContainingScene,
) {
    let Ok(player) = player_emotes.get_single_mut() else {
        return;
    };
    let containing_scenes = containing_scene.get(player);

    for (scene_ent, emotes, parent, container) in &scene_player_emotes {
        if parent.0 == player {
            commands.entity(scene_ent).remove::<EmoteList>();
            if containing_scenes.contains(&container.root) {
                commands.entity(player).insert(emotes.clone());
            }
        }
    }
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
        if let Some(PbAvatarEmoteCommand {
            emote_command: Some(EmoteCommand { emote_urn, .. }),
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
            commands
                .entity(ev.sender)
                .try_insert(EmoteList::new(emote_urn));
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
}

impl Default for ActiveEmote {
    fn default() -> Self {
        Self {
            urn: EmoteUrn::new("idle_male").unwrap(),
            speed: Default::default(),
            restart: Default::default(),
            repeat: Default::default(),
            finished: Default::default(),
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
    )>,
    mut velocities: Local<HashMap<Entity, Vec3>>,
    mut current_emote_min_velocities: Local<HashMap<Entity, f32>>,
    time: Res<Time>,
) {
    let prior_velocities = std::mem::take(&mut *velocities);
    let prior_min_velocities = std::mem::take(&mut *current_emote_min_velocities);

    for (avatar_ent, dynamic_state, mut emotes, gt, active_emote) in avatars.iter_mut() {
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
        let damped_velocity = dynamic_state.velocity * ratio + prior_velocity * (1.0 - ratio);
        let damped_velocity_len = damped_velocity.xz().length();
        velocities.insert(avatar_ent, damped_velocity);

        // get requested emote
        let (mut requested_emote, request_loop) = if let Some(PbAvatarEmoteCommand {
            emote_command: Some(EmoteCommand { emote_urn, r#loop }),
        }) = emote
        {
            (EmoteUrn::new(emote_urn.as_str()).ok(), r#loop)
        } else {
            (None, false)
        };

        // check / cancel requested emote
        if Some(&active_emote.urn) == requested_emote.as_ref() {
            let playing_min_vel = prior_min_velocities.get(&avatar_ent).copied().unwrap_or_default();
            if damped_velocity_len * 0.9 > playing_min_vel {
                // stop emotes on move
                debug!(
                    "clear on motion {} > {}",
                    damped_velocity_len, playing_min_vel
                );
                requested_emote = None;
            } else {
                current_emote_min_velocities.insert(avatar_ent, damped_velocity_len.min(playing_min_vel));
            }
        } else {
            current_emote_min_velocities.insert(avatar_ent, damped_velocity_len);
        }

        // clear given emotes if finished or cancelled
        if active_emote.finished || requested_emote.is_none() {
            if let Some(emotes) = emotes.as_mut() {
                emotes.clear();
            }
            requested_emote = None;
        };

        // play requested emote
        if let Some(requested_emote) = requested_emote {
            *active_emote = ActiveEmote {
                urn: requested_emote,
                speed: 1.0,
                restart: emotes_changed,
                repeat: request_loop,
                finished: false,
            };
            continue;
        }

        // otherwise play a default emote baesd on motion
        if dynamic_state.ground_height > 0.2 {
            *active_emote = ActiveEmote {
                urn: EmoteUrn::new("jump").unwrap(),
                speed: 1.25,
                restart: dynamic_state.velocity.y > 0.0,
                repeat: true,
                finished: false,
            };
            continue;
        }

        let directional_velocity_len = (damped_velocity * (Vec3::X + Vec3::Z)).dot(gt.forward());

        if damped_velocity_len.abs() > 0.1 {
            if damped_velocity_len.abs() < 2.0 {
                *active_emote = ActiveEmote {
                    urn: EmoteUrn::new("walk").unwrap(),
                    speed: directional_velocity_len / 1.5,
                    restart: false,
                    repeat: true,
                    finished: false,
                };
            } else {
                *active_emote = ActiveEmote {
                    urn: EmoteUrn::new("run").unwrap(),
                    speed: directional_velocity_len / 4.5,
                    restart: false,
                    repeat: true,
                    finished: false,
                };
            }
        } else {
            *active_emote = ActiveEmote {
                urn: EmoteUrn::new("idle_male").unwrap(),
                speed: 1.0,
                restart: false,
                repeat: true,
                finished: false,
            };
        }
    }
}

fn play_current_emote(
    mut q: Query<(&mut ActiveEmote, &UserProfile, &AvatarAnimPlayer)>,
    mut emote_loader: CollectibleManager<Emote>,
    gltfs: Res<Assets<Gltf>>,
    animations: Res<Assets<AnimationClip>>,
    mut players: Query<&mut AnimationPlayer>,
    mut playing: Local<HashMap<Entity, EmoteUrn>>,
) {
    let prior_playing = std::mem::take(&mut *playing);

    for (mut active_emote, profile, target_entity) in q.iter_mut() {
        let ent = target_entity.0;
        let bodyshape = base_bodyshapes().remove(if profile.is_female() { 0 } else { 1 });

        // if let Some(scene_emote) = active_emote.urn.scene_emote() {

        // }

        let clip = match emote_loader.get_representation(&active_emote.urn, bodyshape.as_str()) {
            Ok(emote) => {
                let Some(clip) = emote.avatar_animation(&gltfs) else {
                    debug!("{} -> no clip", active_emote.urn);
                    continue;
                };

                clip
            }
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

        let Ok(mut player) = players.get_mut(ent) else {
            continue;
        };

        if Some(&active_emote.urn) != prior_playing.get(&ent) || active_emote.restart {
            player.play_with_transition(clip.clone(), Duration::from_millis(100));
            if active_emote.repeat {
                player.repeat();
            } else {
                player.set_repeat(RepeatAnimation::Never);
            }
        }

        if active_emote.urn.as_str() == "urn:decentraland:off-chain:base-emotes:jump" && player.elapsed() >= 0.75
        {
            player.pause();
        } else {
            player.resume();
        }

        player.set_speed(active_emote.speed);
        playing.insert(ent, active_emote.urn.clone());

        active_emote.restart = false;
        active_emote.finished =
            player.elapsed() >= animations.get(clip).map_or(f32::MAX, |c| c.duration())
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
                .try_insert(EmoteList::new(urn.clone()));
        };
        input.ok();
    }
}
