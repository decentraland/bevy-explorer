use std::{collections::VecDeque, time::Duration};

use bevy::{animation::RepeatAnimation, math::Vec3Swizzles, prelude::*, utils::HashMap};
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

// current manually played emote and min velocity
#[derive(Component, Default)]
struct PlayingEmote(Option<(String, f32)>);

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
                animate.after(process_avatar),
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

// TODO this function is a POS
// lots of magic numbers that don't even deserve to be constants, needs reworking
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn animate(
    mut commands: Commands,
    mut avatars: Query<(
        Entity,
        &AvatarAnimPlayer,
        &AvatarDynamicState,
        Option<&mut EmoteList>,
        Option<&UserProfile>,
        Option<&PlayingEmote>,
        &GlobalTransform,
    )>,
    mut players: Query<&mut AnimationPlayer>,
    mut emote_loader: CollectibleManager<Emote>,
    mut velocities: Local<HashMap<Entity, Vec3>>,
    mut playing: Local<HashMap<Entity, EmoteUrn>>,
    time: Res<Time>,
    anim_assets: Res<Assets<AnimationClip>>,
) {
    let prior_velocities = std::mem::take(&mut *velocities);
    let prior_playing = std::mem::take(&mut *playing);

    let mut play = |anim: &EmoteUrn,
                    speed: f32,
                    ent: Entity,
                    restart: bool,
                    repeat: bool,
                    bodyshape: &str|
     -> bool {
        let clip = match emote_loader.get_representation(anim, bodyshape) {
            Ok(emote) => emote.avatar_animation.clone(),
            Err(CollectibleError::Failed)
            | Err(CollectibleError::Missing)
            | Err(CollectibleError::NoRepresentation) => return true,
            Err(CollectibleError::Loading) => return false,
        };

        let Ok(mut player) = players.get_mut(ent) else {
            return false;
        };

        if Some(anim) != prior_playing.get(&ent) || restart {
            player.play_with_transition(clip.clone(), Duration::from_millis(100));
            if repeat {
                player.repeat();
            } else {
                player.set_repeat(RepeatAnimation::Never);
            }
        }

        if anim.as_str() == "urn:decentraland:off-chain:base-emotes:jump"
            && player.elapsed() >= 0.75
        {
            player.pause();
        } else {
            player.resume();
        }

        player.set_speed(speed);
        playing.insert(ent, anim.to_owned());
        player.elapsed() >= anim_assets.get(clip).map_or(f32::MAX, |c| c.duration())
    };

    for (avatar_ent, animplayer_ent, dynamic_state, mut emotes, profile, maybe_playing_emote, gt) in
        avatars.iter_mut()
    {
        // take a copy of the last entry, remove others
        let mut emote = emotes
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

        let prior_velocity = prior_velocities
            .get(&avatar_ent)
            .copied()
            .unwrap_or(Vec3::ZERO);
        let ratio = time.delta_seconds().clamp(0.0, 0.1) / 0.1;
        let damped_velocity = dynamic_state.velocity * ratio + prior_velocity * (1.0 - ratio);
        let damped_velocity_len = damped_velocity.xz().length();
        velocities.insert(avatar_ent, damped_velocity);

        let mut min_playing_velocity = damped_velocity_len;

        if let Some(PlayingEmote(Some((playing_src, playing_min_vel)))) = maybe_playing_emote {
            if Some(playing_src)
                == emote
                    .as_ref()
                    .and_then(|e| e.emote_command.as_ref())
                    .map(|ec| &ec.emote_urn)
                && damped_velocity_len * 0.9 > *playing_min_vel
            {
                // stop emotes on move
                debug!(
                    "clear on motion {} > {}",
                    damped_velocity_len, playing_min_vel
                );
                if let Some(emotes) = emotes.as_mut() {
                    emotes.clear();
                    emote = None;
                }
                commands.entity(avatar_ent).insert(PlayingEmote::default());
            }
            min_playing_velocity = min_playing_velocity.min(*playing_min_vel);
        }

        let bodyshape = base_bodyshapes().remove(if profile.map_or(true, UserProfile::is_female) {
            0
        } else {
            1
        });

        if let Some(PbAvatarEmoteCommand {
            emote_command: Some(EmoteCommand { emote_urn, r#loop }),
        }) = emote
        {
            if let Ok(emote) = EmoteUrn::new(&emote_urn) {
                if play(&emote, 1.0, animplayer_ent.0, false, r#loop, &bodyshape) && !r#loop {
                    // emote has finished, remove from the set so will resume default anim after
                    emotes.as_mut().unwrap().clear();
                    commands.entity(avatar_ent).insert(PlayingEmote::default());
                } else {
                    commands.entity(avatar_ent).insert(PlayingEmote(Some((
                        emote_urn.to_owned(),
                        min_playing_velocity,
                    ))));
                };
                continue;
            } else {
                warn!("invalid emote from scene: {}", emote_urn);
            }
        }

        if dynamic_state.ground_height > 0.2 {
            play(
                &EmoteUrn::new("jump").unwrap(),
                1.25,
                animplayer_ent.0,
                dynamic_state.velocity.y > 0.0,
                true,
                &bodyshape,
            );
            continue;
        }

        let directional_velocity_len = (damped_velocity * (Vec3::X + Vec3::Z)).dot(gt.forward());

        if damped_velocity_len.abs() > 0.1 {
            if damped_velocity_len.abs() < 2.0 {
                play(
                    &EmoteUrn::new("walk").unwrap(),
                    directional_velocity_len / 1.5,
                    animplayer_ent.0,
                    false,
                    true,
                    &bodyshape,
                );
            } else {
                play(
                    &EmoteUrn::new("run").unwrap(),
                    directional_velocity_len / 4.5,
                    animplayer_ent.0,
                    false,
                    true,
                    &bodyshape,
                );
            }
        } else {
            play(
                &EmoteUrn::new("idle_male").unwrap(),
                1.0,
                animplayer_ent.0,
                false,
                true,
                &bodyshape,
            );
        }
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
