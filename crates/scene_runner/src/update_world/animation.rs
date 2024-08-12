// TODO
// - support blending animations
// - suport morph targets
use bevy::prelude::*;
use bevy::animation::RepeatAnimation;

use common::sets::SceneSets;
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::sdk::components::{PbAnimationState, PbAnimator},
    SceneComponentId,
};

use crate::SceneEntity;

use super::{gltf_container::{Clips, GltfProcessed}, AddCrdtInterfaceExt};

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
        (
            Entity,
            &SceneEntity,
            Option<&mut Animator>,
            &mut AnimationPlayer, &Clips
        ),
        Or<(Changed<Animator>, Changed<GltfProcessed>)>,
    >,
) {
    for (ent, scene_ent, mut maybe_animator, mut player, clips) in animators.iter_mut() {
        let maybe_index = match maybe_animator {
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
                    let Some(index) = clips.named.get(state.clip.as_str()) else {
                        warn!("requested clip {} doesn't exist", state.clip);
                        continue;
                    };
                    Some((*index, state.clone()))
                } else {
                    None
                }
            }
            None => {
                Some((clips.default, PbAnimationState::default()))
            }
        };

        if let Some((index, state)) = maybe_index {
            debug!(
                "[{ent:?}/{scene_ent:?}] playing (something) with state {:?}",
                state
            );

            let active_animation = player.play(index);

            active_animation.set_speed(state.speed.unwrap_or(1.0));
            if state.r#loop.unwrap_or(true) {
                active_animation.repeat();
            } else {
                if state.should_reset.unwrap_or(false) {
                    active_animation.replay();
                }

                active_animation.set_repeat(RepeatAnimation::Never);
            }

            // on my version of bevy animator this means "should go back to starting position when finished"
            // player.set_should_reset(false);

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
            player.pause_all();
        }
    }
}
