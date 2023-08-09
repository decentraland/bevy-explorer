// TODO
// - support blending animations
// - suport morph targets
use bevy::gltf::Gltf;
use bevy::prelude::*;

use common::sets::SceneSets;
use dcl::interface::ComponentPosition;
use dcl_component::{proto_components::sdk::components::PbAnimator, SceneComponentId};

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
        (&mut Animator, &Handle<Gltf>, &GltfProcessed),
        Or<(Changed<Animator>, Changed<GltfProcessed>)>,
    >,
    mut players: Query<&mut AnimationPlayer, &Name>,
    clips: Res<Assets<AnimationClip>>,
    gltfs: Res<Assets<Gltf>>,
) {
    for (mut animator, h_gltf, gltf_processed) in animators.iter_mut() {
        // TODO bevy only supports a single concurrent animation (or a single timed transition which we can't use)
        // it is still in development so will probably have better support soon. otherwise we could build our own
        // animator to handle blending if required.
        // for now, we choose highest weighted animation
        let (_, req_state) = animator
            .pb_animator
            .states
            .iter()
            .fold((0.0, None), |v, state| {
                if !state.playing.unwrap_or_default() {
                    return v;
                }

                let current_weight = v.0;
                let state_weight = state.weight.unwrap_or(1.0);
                if state_weight > current_weight {
                    (state_weight, Some(state))
                } else {
                    v
                }
            });

        if let Some(state) = req_state {
            let Some(gltf) = gltfs.get(h_gltf) else {
                // set change tick on the animator so that we recheck next frame
                // TODO this will recheck forever if the gltf fails to load
                animator.set_changed();
                continue;
            };

            let Some(h_clip) = gltf.named_animations.get(&state.clip) else {
                warn!("requested clip {} doesn't exist", state.clip);
                continue;
            };
            let Some(clip) = clips.get(h_clip) else {
                // set change tick on the animator so that we recheck next frame
                // TODO this will recheck forever if the gltf fails to load
                animator.set_changed();
                continue;
            };

            // bevy adds a player to each animated root node.
            // we can't track which root node corresponds to which animation.
            // in gltfs, the animation nodes must be uniquely named so we
            // can just add the animation to every player with the right name.
            for (player_ent, _) in gltf_processed.animation_roots.iter().filter(|(_, name)| clip.compatible_with(name)) {
                let mut player = players.get_mut(*player_ent).unwrap();

                player.play(h_clip.clone_weak());
                player.set_speed(state.speed.unwrap_or(1.0));
                if state.r#loop.unwrap_or(true) {
                    player.repeat();
                } else {
                    player.stop_repeating();
                }
            }

            animator.bypass_change_detection().playing = true;
        } else if animator.playing {
            animator.bypass_change_detection().playing = false;
            for (player_ent, _) in gltf_processed.animation_roots.iter() {
                let mut player = players.get_mut(*player_ent).unwrap();

                player.pause();
            }
        }
    }
}
