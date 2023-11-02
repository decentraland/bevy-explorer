use bevy::{prelude::*, math::Vec3Swizzles};
use common::structs::PrimaryUser;
use comms::global_crdt::ForeignPlayer;
use ethers_core::rand::{thread_rng, Rng, seq::SliceRandom};
use scene_runner::{
    initialize_scene::{SceneHash, SceneLoading, ScenePointers, PARCEL_SIZE, PointerResult, LiveScenes},
    renderer_context::RendererSceneContext,
    OutOfWorld,
};

pub fn handle_out_of_world(
    mut commands: Commands,
    scenes: Query<(Option<&RendererSceneContext>, Option<&SceneLoading>), With<SceneHash>>,
    mut player: Query<(Entity, &mut Transform), (With<PrimaryUser>, With<OutOfWorld>)>,
    pointers: Res<ScenePointers>,
    live_scenes: Res<LiveScenes>,
    foreign_players: Query<&GlobalTransform, With<ForeignPlayer>>,
) {
    let Ok((player, mut t)) = player.get_single_mut() else {
        return;
    };

    debug!("out of world!");

    let parcel = (t.translation.xz() * Vec2::new(1.0, -1.0) / PARCEL_SIZE)
        .floor()
        .as_ivec2();

    let hash = match pointers.0.get(&parcel) {
        Some(PointerResult::Exists(hash)) => hash,
        Some(PointerResult::Nothing(_, _)) => {
            debug!("scene {parcel} doesn't exist, returning to world");
            debug!("everything: {:?}", pointers.0);
            commands.entity(player).remove::<OutOfWorld>();
            return;            
        },
        None => {
            // we don't know yet, the scene isn't loaded
            debug!("waiting for scene to resolve");
            return;
        }
    };

    let Some(scene) = live_scenes.0.get(hash) else {
        debug!("scene resolved but not spawned");
        return;
    };

    let (maybe_context, maybe_loadstate) = scenes.get(*scene).unwrap();

    if let Some(context) = maybe_context {
        if context.tick_number <= 5 || !context.blocked.is_empty() {
            debug!("scene not ready");
        } else {
            debug!("ready, returning to world (set spawn here) tick: {}", context.tick_number);

            let other_positions = foreign_players.iter().map(|gt| gt.translation()).collect::<Vec<_>>();
            let base_position = Vec3::new(context.base.x as f32, 0.0, -context.base.y as f32) * PARCEL_SIZE;

            let rng = &mut thread_rng();
            let mut best_distance = 0.0;
            let mut best_position = Vec3::new(rng.gen_range(0.0..PARCEL_SIZE), rng.gen_range(0.0..PARCEL_SIZE), rng.gen_range(0.0..PARCEL_SIZE));
            let mut count = 100;

            if context.spawn_points.len() > 0 {
                while best_distance < 0.75 && count > 0 {
                    let spawn_point = context.spawn_points.choose(rng).unwrap();
                    let aabb = spawn_point.position.bounding_box();
                    let position = base_position + Vec3::new(rng.gen_range(aabb.0.x..=aabb.1.x), rng.gen_range(aabb.0.y..=aabb.1.y), -rng.gen_range(aabb.0.z..=aabb.1.z));
                    let distance = other_positions.iter().fold(0.75, |d, other| f32::min(d, (position - *other).length()));
                    if distance > best_distance {
                        best_distance = distance;
                        best_position = position;
                    }

                    count -= 1;
                }
            }

            debug!("chose {best_position}");
            t.translation = best_position;
            commands.entity(player).remove::<OutOfWorld>();
        }
        return;
    }

    match maybe_loadstate {
        Some(SceneLoading::Failed) => {
            debug!("failed, returning to world");
            commands.entity(player).remove::<OutOfWorld>();
            return;
        }
        Some(_) => {
            debug!("scene not loaded");
            return;
        }
        None => {
            panic!("no context or loadstate?!");
        }
    }
}
