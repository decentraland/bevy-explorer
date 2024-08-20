// TODO
// - support blending animations
// - suport morph targets
use bevy::{animation::RepeatAnimation, utils::hashbrown::HashSet};
use bevy::{prelude::*, utils::HashMap};

use common::sets::SceneSets;
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::sdk::components::{PbAnimationState, PbAnimator},
    SceneComponentId,
};
use petgraph::graph::NodeIndex;

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

#[derive(Component, Default)]
pub struct Clips {
    pub default: Option<NodeIndex>,
    pub named: HashMap<String, (NodeIndex, f32)>,
}

#[derive(Component)]
pub struct Animator {
    pb_animator: PbAnimator,
}

impl From<PbAnimator> for Animator {
    fn from(pb_animator: PbAnimator) -> Self {
        Self { pb_animator }
    }
}

#[allow(clippy::type_complexity)]
fn update_animations(
    mut animators: Query<
        (
            Entity,
            &SceneEntity,
            Option<&mut Animator>,
            &mut AnimationPlayer,
            &Clips,
        ),
        Or<(Changed<Animator>, Changed<GltfProcessed>)>,
    >,
) {
    for (ent, scene_ent, maybe_animator, mut player, clips) in animators.iter_mut() {
        debug!(
            "[{ent:?} / {scene_ent:?}] {:?}",
            maybe_animator.as_ref().map(|a| &a.pb_animator)
        );
        let targets: HashMap<AnimationNodeIndex, (f32, PbAnimationState)> = match maybe_animator {
            Some(ref animator) => animator
                .pb_animator
                .states
                .iter()
                .filter_map(|state| {
                    clips
                        .named
                        .get(state.clip.as_str())
                        .map(|(index, duration)| (*index, (*duration, state.clone())))
                })
                .collect(),
            None => clips
                .default
                .map(|clip| (clip, (0.0, PbAnimationState::default())))
                .into_iter()
                .collect(),
        };

        let mut prev_anims: HashSet<_> = player.playing_animations().map(|(ix, _)| *ix).collect();

        for (ix, (duration, state)) in targets.into_iter() {
            let playing = state.playing.unwrap_or(true);
            let new_weight = if !playing {
                0.0
            } else {
                state.weight.unwrap_or(1.0)
            };
            let new_speed = if !playing {
                0.0
            } else {
                state.speed.unwrap_or(1.0)
            };

            let active_animation = match (prev_anims.remove(&ix), state.should_reset()) {
                // if shouldReset, we always (re)start
                (_, true) |
                // if not playing we start
                (false, _) => {
                    let anim = player.start(ix);

                    if new_speed < 0.0 {
                        anim.seek_to(duration);
                    }
                    anim
                }
                // otherwise use existing
                (true, false) => player.animation_mut(ix).unwrap(),
            };

            active_animation.set_weight(new_weight);
            active_animation.set_speed(new_speed);
            if state.r#loop.unwrap_or(true) {
                active_animation.repeat();
            } else {
                active_animation.set_repeat(RepeatAnimation::Never);
            }

            // clamp seek time and reset completions
            if duration != 0.0 {
                let seek_time = active_animation.seek_time().clamp(0.0, duration);
                active_animation.replay();
                active_animation.seek_to(seek_time);
            }
        }

        let playing = player.playing_animations().collect::<Vec<_>>();
        debug!("final: {:?}", playing);

        // stop anims that have been removed
        for ix in prev_anims {
            player.stop(ix);
        }
    }
}
