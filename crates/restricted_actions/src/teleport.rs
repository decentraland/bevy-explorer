use bevy::{math::Vec3Swizzles, prelude::*};
use common::{
    rpc::{RpcCall, RpcResultSender},
    structs::{AvatarDynamicState, CurrentRealm, PermissionType, PrimaryUser},
};
use comms::global_crdt::ForeignPlayer;
use ethers_core::rand::{seq::SliceRandom, thread_rng, Rng};
use ipfs::{ChangeRealmEvent, RealmInitialLocation};
use scene_runner::{
    initialize_scene::{
        LiveScenes, PointerResult, SceneHash, SceneLoading, ScenePointers, PARCEL_SIZE,
    },
    permissions::Permission,
    renderer_context::{RendererSceneContext, FROZEN_BLOCK},
    update_world::{gltf_container::GLTF_LOADING, mesh_collider::SceneColliderData},
    OutOfWorld,
};
use wallet::Wallet;

type TeleportAction = (IVec2, Option<String>, RpcResultSender<Result<(), String>>);

pub fn teleport_player(
    mut commands: Commands,
    mut events: EventReader<RpcCall>,
    mut player: Query<(Entity, &mut Transform, &mut AvatarDynamicState), With<PrimaryUser>>,
    mut perms: Permission<TeleportAction>,
    mut realm_target: ResMut<RealmInitialLocation>,
) {
    let mut actions: Vec<TeleportAction> = Vec::new();

    for (scene, to, realm, response) in events.read().filter_map(|ev| match ev {
        RpcCall::TeleportPlayer {
            scene,
            to,
            realm,
            response,
        } => Some((*scene, *to, realm.clone(), response.clone())),
        _ => None,
    }) {
        let Some(scene) = scene else {
            actions.push((to, realm, response));
            continue;
        };
        let (ty, detail) = match &realm {
            Some(realm) => (
                PermissionType::ChangeRealm,
                format!("{realm} ({},{})", to.x, to.y),
            ),
            None => (PermissionType::Teleport, format!("({},{})", to.x, to.y)),
        };
        perms.check(ty, scene, (to, realm, response), Some(detail), false);
    }

    actions.extend(perms.drain_success(PermissionType::Teleport));
    actions.extend(perms.drain_success(PermissionType::ChangeRealm));

    for (to, realm, response) in actions {
        if let Some(realm) = realm {
            // A realm change (a full reconnect, even to the realm we are in — same as changeRealm),
            // landing on the parcel once it is live. It can't be applied now: it would resolve
            // against the parcel grid of the realm being left.
            debug!("teleport -> parcel {to} in {realm}");
            *realm_target = RealmInitialLocation::Parcel(to);
            commands.send_event(ChangeRealmEvent {
                new_realm: realm,
                content_server_override: None,
            });
            response.send(Ok(()));
            continue;
        }

        let Ok((ent, mut transform, mut dynamic_state)) = player.single_mut() else {
            warn!("player doesn't exist?!");
            response.send(Err("Something went wrong".into()));
            continue;
        };

        transform.translation.x = to.x as f32 * 16.0 + 8.0;
        transform.translation.z = -to.y as f32 * 16.0 - 8.0;
        dynamic_state.velocity = Vec3::ZERO;
        if let Ok(mut commands) = commands.get_entity(ent) {
            commands.try_insert(OutOfWorld);
        }

        debug!("teleport -> none");
        *realm_target = RealmInitialLocation::None;

        response.send(Ok(()));
        info!("teleported to {to}");
    }

    for (_, _, response) in perms.drain_fail(PermissionType::Teleport) {
        response.send(Err("User declined".to_owned()))
    }
    for (_, _, response) in perms.drain_fail(PermissionType::ChangeRealm) {
        response.send(Err("User declined".to_owned()))
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
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
    current_realm: Res<CurrentRealm>,
) {
    let Ok((player, mut t)) = player.single_mut() else {
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

    let hash = match pointers.get(parcel) {
        // Only trust a pointer tagged with the realm we are actually in. A pointer from the realm
        // we just left survives a realm change (`process_realm_change` reconciles pointers against
        // the new realm's scene list, which an ActiveEntities realm like Genesis doesn't have, so
        // nothing is purged there) and `load_active_entities` treats such a pointer as "must
        // re-request" rather than as an answer — see its `realm != current_realm.pointer_realm()`
        // filter. Trusting it here let a teleport into a fresh realm resolve to the PREVIOUS
        // realm's scene, which is already ticking and "ready", so the player was dropped straight
        // into it: no loading screen, no asset count, and the real scene streamed in afterwards.
        // Treat it like an unresolved parcel and wait for the sweep to answer for this realm.
        Some(PointerResult::Exists { realm, hash, .. })
            if realm == current_realm.pointer_realm() =>
        {
            hash
        }
        Some(PointerResult::Exists { .. }) => {
            debug!("scene {parcel} is from another realm, waiting for this realm to resolve it");
            return;
        }
        Some(PointerResult::Nothing) => {
            debug!("scene {parcel} doesn't exist, returning to world");
            debug!("everything: {:?}", pointers);
            commands.entity(player).remove::<OutOfWorld>();
            return;
        }
        None => {
            // we don't know yet, the scene isn't loaded
            debug!("waiting for scene to resolve");
            return;
        }
    };

    let Some(scene) = live_scenes.scenes.get(hash) else {
        debug!("scene resolved but not spawned");
        return;
    };

    let (maybe_context, maybe_loadstate, maybe_collider_data) = scenes.get_mut(*scene).unwrap();

    if let Some(context) = maybe_context {
        // A frozen scene (inspector /freeze_scene, or the editor-mode auto-freeze at tick 3) is as
        // loaded as it's going to get — it never advances its tick or clears `blocked` to become
        // "ready" on its own, so don't keep the player out-of-world behind the loading screen. Do
        // still wait for gltfs though: they keep processing renderer-side while the scene is frozen,
        // and releasing early would drop the player in before the ground colliders exist.
        let frozen =
            context.blocked.contains(FROZEN_BLOCK) && !context.blocked.contains(GLTF_LOADING);
        // A broken scene (hung past the not-responding timeout, errored, or unreachable) will
        // never become ready; stop holding the player behind the loading screen and let them
        // into the world. An inspected scene never gates the player at all: it's a debugging
        // session, and it may sit at a breakpoint (or paused before its first tick) forever.
        if !context.broken()
            && !frozen
            && !context.inspected
            && (context.tick_number <= 5 || !context.blocked.is_empty())
        {
            debug!("scene not ready");
        } else {
            if context.broken() {
                debug!("scene broken, returning to world");
            }
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
                    .and_then(|mut cd| cd.get_ground(best_position))
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
