use std::time::Duration;

use bevy::{gltf::Gltf, math::Vec3Swizzles, prelude::*, utils::HashMap};

use super::AvatarDynamicState;

#[derive(Resource, Default)]
pub struct AvatarAnimations(pub HashMap<String, Handle<AnimationClip>>);

#[derive(Component)]
pub struct AvatarAnimPlayer(pub Entity);

pub struct AvatarAnimationPlugin;

impl Plugin for AvatarAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems((load_animations, animate));
        app.init_resource::<AvatarAnimations>();
    }
}

#[allow(clippy::type_complexity)]
fn load_animations(
    asset_server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    mut builtin_animations: Local<Option<Vec<Handle<Gltf>>>>,
    mut animations: ResMut<AvatarAnimations>,
) {
    if builtin_animations.is_none() {
        *builtin_animations = Some(vec![
            asset_server.load("animations/walk.glb"),
            asset_server.load("animations/idle.glb"),
            asset_server.load("animations/run.glb"),
            asset_server.load("animations/jump.glb"),
        ]);
    } else {
        builtin_animations.as_mut().unwrap().retain(|h_gltf| {
            match gltfs.get(h_gltf).map(|gltf| &gltf.named_animations) {
                Some(anims) => {
                    for (name, h_clip) in anims {
                        animations.0.insert(name.clone(), h_clip.clone());
                        debug!("added animation {name}");
                    }
                    false
                }
                None => true,
            }
        })
    }
}

// TODO this function is a POS
fn animate(
    avatars: Query<(Entity, &AvatarAnimPlayer, &AvatarDynamicState)>,
    mut players: Query<&mut AnimationPlayer>,
    animations: Res<AvatarAnimations>,
    mut velocities: Local<HashMap<Entity, Vec3>>,
    mut playing: Local<HashMap<Entity, &str>>,
    time: Res<Time>,
) {
    let prior_velocities = std::mem::take(&mut *velocities);
    let prior_playing = std::mem::take(&mut *playing);

    let mut play = |anim: &'static str, speed: f32, ent: Entity, restart: bool| {
        if let Some(clip) = animations.0.get(anim) {
            if let Ok(mut player) = players.get_mut(ent) {
                if restart && player.elapsed() == 0.75 {
                    player.start(clip.clone()).repeat();
                } else if Some(&anim) != prior_playing.get(&ent) || restart {
                    player
                        .play_with_transition(clip.clone(), Duration::from_millis(100))
                        .repeat();
                }

                if anim == "Jump" && player.elapsed() >= 0.75 {
                    player.pause();
                } else {
                    player.resume();
                }

                player.set_speed(speed);
                playing.insert(ent, anim);
            }
        }
    };

    for (avatar_ent, animplayer_ent, dynamic_state) in avatars.iter() {
        let prior_velocity = prior_velocities
            .get(&avatar_ent)
            .copied()
            .unwrap_or(Vec3::ZERO);
        let ratio = time.delta_seconds().clamp(0.0, 0.1) / 0.1;
        let damped_velocity = dynamic_state.velocity * ratio + prior_velocity * (1.0 - ratio);

        if dynamic_state.ground_height > 0.2 {
            play(
                "Jump",
                1.25,
                animplayer_ent.0,
                dynamic_state.velocity.y > 0.0,
            );
            continue;
        }

        let damped_velocity_len = damped_velocity.xz().length();
        if damped_velocity_len > 0.1 {
            if damped_velocity_len < 2.0 {
                play("Walk", damped_velocity_len / 1.5, animplayer_ent.0, false);
            } else {
                play("Run", damped_velocity_len / 4.5, animplayer_ent.0, false);
            }
        } else {
            play("Idle_Male", 1.0, animplayer_ent.0, false);
        }

        velocities.insert(avatar_ent, damped_velocity);
    }
}
