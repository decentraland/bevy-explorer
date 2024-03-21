// TODO
// - support blending animations
// - suport morph targets
use bevy::prelude::*;
use bevy::{animation::RepeatAnimation, gltf::Gltf};

use common::sets::SceneSets;
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::sdk::components::{PbAnimationState, PbAnimator},
    SceneComponentId,
};

use crate::SceneEntity;

use super::{gltf_container::GltfProcessed, AddCrdtInterfaceExt};

pub struct AnimatorPlugin;

impl Plugin for AnimatorPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbAnimator, Animator>(
            SceneComponentId::ANIMATOR,
            ComponentPosition::EntityOnly,
        );

        app.add_systems(Update, update_animations.in_set(SceneSets::PostLoop));
    }
}

#[derive(Component)]
pub struct Animator {
    pb_animator: PbAnimator,
    playing: bool,
}

impl From<PbAnimator> for Animator {
    fn from(pb_animator: PbAnimator) -> Self {
        Self {
            pb_animator,
            playing: false,
        }
    }
}

#[allow(clippy::type_complexity)]
fn update_animations(
    mut animators: Query<
        (Entity, &SceneEntity, Option<&mut Animator>, &Handle<Gltf>, &mut GltfProcessed),
        Or<(Changed<Animator>, Changed<GltfProcessed>)>,
    >,
    mut players: Query<&mut AnimationPlayer>,
    clips: Res<Assets<AnimationClip>>,
    gltfs: Res<Assets<Gltf>>,
) {
    for (ent, scene_ent, mut maybe_animator, h_gltf, mut gltf_processed) in animators.iter_mut() {
        let maybe_h_clip = match maybe_animator {
            Some(ref animator) => {
                // TODO bevy only supports a single concurrent animation (or a single timed transition which we can't use)
                // it is still in development so will probably have better support soon. otherwise we could build our own
                // animator to handle blending if required.
                // for now, we choose highest weighted animation
                let (_, req_state) =
                    animator
                        .pb_animator
                        .states
                        .iter()
                        .fold((0.0, None), |v, state| {
                            if !state.playing.unwrap_or_default() {
                                return v;
                            }

                            let current_weight = v.0;
                            let state_weight = state.weight.unwrap_or(1.0);
                            if state_weight >= current_weight {
                                (state_weight, Some(state))
                            } else {
                                v
                            }
                        });

                if let Some(state) = req_state {
                    let Some(gltf) = gltfs.get(h_gltf) else {
                        // set change tick on the animator so that we recheck next frame
                        // TODO this will recheck forever if the gltf fails to load
                        gltf_processed.set_changed();
                        continue;
                    };

                    let Some(h_clip) = gltf.named_animations.get(&state.clip) else {
                        warn!("requested clip {} doesn't exist", state.clip);
                        continue;
                    };
                    Some((h_clip, state.clone()))
                } else {
                    None
                }
            }
            None => {
                // if no animator is present we should play the first clip, if any exist
                let Some(gltf) = gltfs.get(h_gltf) else {
                    // set change tick on the animator so that we recheck next frame
                    // TODO this will recheck forever if the gltf fails to load
                    gltf_processed.set_changed();
                    continue;
                };

                gltf.animations
                    .first()
                    .map(|anim| (anim, PbAnimationState::default()))
            }
        };

        if let Some((h_clip, state)) = maybe_h_clip {
            let Some(clip) = clips.get(h_clip) else {
                // set change tick on the animator so that we recheck next frame
                // TODO this will recheck forever if the gltf fails to load
                gltf_processed.set_changed();
                continue;
            };

            // bevy adds a player to each animated root node.
            // we can't track which root node corresponds to which animation.
            // in gltfs, the animation nodes must be uniquely named so we
            // can just add the animation to every player with the right name.
            let (target, others) = gltf_processed
                .animation_roots
                .iter()
                .partition::<Vec<_>, _>(|(_, name)| clip.compatible_with(name));

            if target.is_empty() {
                warn!("invalid root node for animation: (there is no name field any more)");
                warn!(
                    "available root nodes: {:?}",
                    gltf_processed
                        .animation_roots
                        .iter()
                        .map(|(_, name)| name.as_str())
                        .collect::<Vec<_>>()
                );
            }

            for (player_ent, _) in target {
                let Ok(mut player) = players.get_mut(*player_ent) else {
                    error!("failed to get animation player");
                    continue;
                };

                debug!("[{ent:?}/{scene_ent:?}] playing (something) with state {:?}", state);
                player.play(h_clip.clone_weak());

                player.set_speed(state.speed.unwrap_or(1.0));
                if state.r#loop.unwrap_or(true) {
                    player.repeat();
                } else {
                    // ok i'm really guessing here, not sure what criteria we should use to force a reset
                    // force restart if speed is not specified??
                    if state.speed.is_none() {
                        player.replay();
                    }

                    player.set_repeat(RepeatAnimation::Never);
                }

                // deprecate should_reset
                player.set_should_reset(false);
            }

            for (player_ent, _) in others {
                let mut player = players.get_mut(*player_ent).unwrap();
                player.pause();
            }

            if let Some(animator) = maybe_animator.as_mut() {
                animator.bypass_change_detection().playing = true;
            }
        } else if maybe_animator
            .as_ref()
            .map_or(false, |animator| animator.playing)
        {
            if let Some(animator) = maybe_animator.as_mut() {
                animator.bypass_change_detection().playing = false;
            }
            for (player_ent, _) in gltf_processed.animation_roots.iter() {
                let mut player = players.get_mut(*player_ent).unwrap();

                player.pause();
            }
        }
    }
}
