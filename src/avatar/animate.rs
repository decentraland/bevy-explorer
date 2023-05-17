use bevy::{gltf::Gltf, prelude::*, utils::HashMap};

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

fn animate(
    avatars: Query<(Entity, &Transform, &AvatarAnimPlayer)>,
    mut players: Query<&mut AnimationPlayer>,
    animations: Res<AvatarAnimations>,
    mut positions: Local<HashMap<Entity, Vec3>>,
) {
    let (Some(idle), Some(walk)) = (animations.0.get("Idle_Male"), animations.0.get("Walk")) else {
        return;
    };

    let prior_positions = std::mem::take(&mut *positions);

    for (avatar_ent, avatar_pos, animplayer_ent) in avatars.iter() {
        let changed = prior_positions
            .get(&avatar_ent)
            .map_or(true, |prior| *prior != avatar_pos.translation);

        if let Ok(mut player) = players.get_mut(animplayer_ent.0) {
            if changed {
                player.play(walk.clone()).repeat();
            } else {
                player.play(idle.clone()).repeat();
            }
        }

        positions.insert(avatar_ent, avatar_pos.translation);
    }
}
