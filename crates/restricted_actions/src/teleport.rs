use avatar::AvatarDynamicState;
use bevy::{ecs::system::RunSystemOnce, math::Vec3Swizzles, prelude::*};
use bevy_dui::{DuiCommandsExt, DuiProps, DuiRegistry};
use common::{rpc::RpcCall, structs::PrimaryUser};
use comms::global_crdt::ForeignPlayer;
use ethers_core::rand::{seq::SliceRandom, thread_rng, Rng};
use scene_runner::{
    initialize_scene::{
        LiveScenes, PointerResult, SceneHash, SceneLoading, ScenePointers, PARCEL_SIZE,
    },
    renderer_context::RendererSceneContext,
    update_world::mesh_collider::SceneColliderData,
    ContainingScene, OutOfWorld,
};
use ui_core::button::DuiButton;
use wallet::Wallet;

pub fn teleport_player(
    mut commands: Commands,
    mut events: EventReader<RpcCall>,
    player: Query<(Entity, &Transform), With<PrimaryUser>>,
    containing_scene: ContainingScene,
    dui: Res<DuiRegistry>,
) {
    for (requester, parcel, response) in events.read().filter_map(|ev| match ev {
        RpcCall::TeleportPlayer {
            scene,
            to,
            response,
        } => Some((*scene, *to, response.clone())),
        _ => None,
    }) {
        if let Some(requester) = requester {
            if !player.get_single().ok().map_or(false, |(e, ..)| {
                containing_scene.get(e).contains(&requester)
            }) {
                warn!("invalid teleport request from non-containing scene");
                warn!("request from {requester:?}");
                warn!(
                    "containing scenes {:?}",
                    player.get_single().map(|p| containing_scene.get(p.0))
                );
                return;
            }
        }

        let response_fail = response.clone();

        let do_teleport = move |mut commands: Commands,
                                mut player: Query<
            (Entity, &mut Transform, &mut AvatarDynamicState),
            With<PrimaryUser>,
        >| {
            let Ok((ent, mut transform, mut dynamic_state)) = player.get_single_mut() else {
                warn!("player doesn't exist?!");
                response.send(Err("Something went wrong".into()));
                return;
            };

            transform.translation.x = parcel.x as f32 * 16.0 + 8.0;
            transform.translation.z = -parcel.y as f32 * 16.0 - 8.0;
            dynamic_state.velocity = Vec3::ZERO;
            if let Some(mut commands) = commands.get_entity(ent) {
                commands.try_insert(OutOfWorld);
            }

            response.send(Ok(()));
            info!("teleported to {parcel}");
        };

        if requester.is_some() {
            commands.spawn_template(
                &dui,
                "text-dialog",
                DuiProps::new().with_prop("title", "Teleport".to_owned())
                    .with_prop("body", format!("The scene wants to teleport you to another location: {},{}\ntodo: put scene name and thumbnail here", parcel.x, parcel.y))
                    .with_prop("buttons", vec![
                        DuiButton::new_enabled_and_close("Let's go!", do_teleport),
                        DuiButton::new_enabled_and_close("No thanks", move || {
                            response_fail.send(Err("User said no thanks".into()));
                        }),
                    ]),
            ).unwrap();
        } else {
            commands.add(|w: &mut World| w.run_system_once(do_teleport))
        }
    }
}

#[allow(clippy::type_complexity)]
pub fn handle_out_of_world(
    mut commands: Commands,
    mut scenes: Query<
        (
            Option<&RendererSceneContext>,
            Option<&SceneLoading>,
            Option<&mut SceneColliderData>,
        ),
        With<SceneHash>,
    >,
    mut player: Query<(Entity, &mut Transform), (With<PrimaryUser>, With<OutOfWorld>)>,
    pointers: Res<ScenePointers>,
    live_scenes: Res<LiveScenes>,
    foreign_players: Query<&GlobalTransform, With<ForeignPlayer>>,
    wallet: Res<Wallet>,
) {
    let Ok((player, mut t)) = player.get_single_mut() else {
        return;
    };

    debug!("out of world!");

    if wallet.address().is_none() {
        debug!("waiting for connection");
        return;
    }

    let parcel = (t.translation.xz() * Vec2::new(1.0, -1.0) / PARCEL_SIZE)
        .floor()
        .as_ivec2();

    let hash = match pointers.0.get(&parcel) {
        Some(PointerResult::Exists { hash, .. }) => hash,
        Some(PointerResult::Nothing { .. }) => {
            debug!("scene {parcel} doesn't exist, returning to world");
            debug!("everything: {:?}", pointers.0);
            commands.entity(player).remove::<OutOfWorld>();
            return;
        }
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

    let (maybe_context, maybe_loadstate, maybe_collider_data) = scenes.get_mut(*scene).unwrap();

    if let Some(context) = maybe_context {
        if !context.broken && (context.tick_number <= 5 || !context.blocked.is_empty()) {
            debug!("scene not ready");
        } else {
            debug!(
                "ready, returning to world (set spawn here) tick: {}",
                context.tick_number
            );

            let other_positions = foreign_players
                .iter()
                .map(|gt| gt.translation())
                .collect::<Vec<_>>();
            let base_position =
                Vec3::new(context.base.x as f32, 0.0, -context.base.y as f32) * PARCEL_SIZE;

            let rng = &mut thread_rng();
            let mut best_distance = 0.0;
            let mut best_position = Vec3::new(
                rng.gen_range(0.0..PARCEL_SIZE),
                1000.0,
                -rng.gen_range(0.0..PARCEL_SIZE),
            ) + base_position;
            best_position.y = 1000.0
                - maybe_collider_data
                    .and_then(|mut cd| cd.get_groundheight(context.tick_number, best_position))
                    .map(|(h, _)| h)
                    .unwrap_or(1000.0);
            let mut count = 100;

            if !context.spawn_points.is_empty() {
                while best_distance < 0.75 && count > 0 {
                    let spawn_point = context.spawn_points.choose(rng).unwrap();
                    if !spawn_point.default && count > 50 {
                        continue;
                    }
                    let aabb = spawn_point.position.bounding_box();
                    let position = base_position
                        + Vec3::new(
                            rng.gen_range(aabb.0.x..=aabb.1.x),
                            rng.gen_range(aabb.0.y..=aabb.1.y),
                            -rng.gen_range(aabb.0.z..=aabb.1.z),
                        );
                    let distance = other_positions
                        .iter()
                        .fold(0.75, |d, other| f32::min(d, (position - *other).length()));
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
        }
        Some(_) => {
            debug!("scene not loaded");
        }
        None => {
            panic!("no context or loadstate?!");
        }
    }
}
