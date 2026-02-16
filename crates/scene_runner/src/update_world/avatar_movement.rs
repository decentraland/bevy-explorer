use std::f32::consts::TAU;

use bevy::{platform::collections::HashMap, prelude::*};
use common::{
    anim_last_system,
    dynamics::{PLAYER_COLLIDER_OVERLAP, PLAYER_COLLIDER_RADIUS, PLAYER_GROUND_THRESHOLD},
    sets::SceneSets,
    structs::{AvatarDynamicState, PrimaryPlayerRes, PrimaryUser},
};
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::{
        common::Vector3,
        sdk::components::{ColliderLayer, PbAvatarMovement},
    },
    SceneComponentId, SceneEntityId,
};

use crate::{
    renderer_context::RendererSceneContext,
    update_world::{
        gltf_container::GltfLinkSet,
        mesh_collider::{
            update_collider_transforms, ColliderId, CtCollider, PreviousColliderTransform,
            SceneColliderData, GROUND_COLLISION_MASK,
        },
        transform_and_parent::{parent_position_sync, AvatarAttachStage, SceneProxyStage},
        AddCrdtInterfaceExt,
    },
    ContainingScene, SceneEntity,
};

pub struct AvatarMovementPlugin;

impl Plugin for AvatarMovementPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbAvatarMovement, AvatarMovement>(
            SceneComponentId::AVATAR_MOVEMENT,
            ComponentPosition::EntityOnly,
        );

        app.add_systems(
            PostUpdate,
            (pick_movement, apply_movement)
                .chain()
                .after(anim_last_system!())
                .after(GltfLinkSet)
                .before(parent_position_sync::<AvatarAttachStage>)
                .before(parent_position_sync::<SceneProxyStage>)
                .before(TransformSystem::TransformPropagate),
        );

        // record ground
        app.add_systems(
            Update,
            (record_ground_collider)
                .in_set(SceneSets::PostInit)
                .before(update_collider_transforms::<CtCollider>),
        );

        // resolve position
        app.add_systems(
            Update,
            (
                apply_ground_collider_movement,
                // apply_pseudo_ground_collider_movement,
                resolve_collisions,
            )
                .chain()
                .in_set(SceneSets::PostInit)
                .after(update_collider_transforms::<CtCollider>),
        );
    }
}

#[derive(Component, Clone, Copy)]
pub struct AvatarMovement {
    pub velocity: Vec3,
    pub orientation: f32,
    pub ground_direction: Vec3,
}

impl Default for AvatarMovement {
    fn default() -> Self {
        Self {
            velocity: Vec3::ZERO,
            orientation: 0.0,
            ground_direction: Vec3::NEG_Y,
        }
    }
}

impl From<PbAvatarMovement> for AvatarMovement {
    fn from(value: PbAvatarMovement) -> Self {
        Self {
            velocity: value.velocity.unwrap_or_default().world_vec_to_vec3(),
            orientation: value.orientation,
            ground_direction: value
                .ground_direction
                .as_ref()
                .map(Vector3::world_vec_to_vec3)
                .map(Vec3::normalize_or_zero)
                .unwrap_or(Vec3::NEG_Y),
        }
    }
}

#[derive(Component)]
pub struct Movement {
    scene: Entity,
    scene_last_update: u32,
    scene_start_tick: u32,
    scene_is_portable: bool,
    movement: AvatarMovement,
}

impl Default for Movement {
    fn default() -> Self {
        Self {
            scene: Entity::PLACEHOLDER,
            scene_last_update: 0,
            scene_start_tick: 0,
            scene_is_portable: true,
            movement: Default::default(),
        }
    }
}

// choose the movement we want to use
fn pick_movement(
    mut commands: Commands,
    q: Query<(&AvatarMovement, &SceneEntity), Changed<AvatarMovement>>,
    scenes: Query<&RendererSceneContext>,
    containing_scenes: ContainingScene,
    mut player: Query<&mut Movement, With<PrimaryUser>>,
    player_res: Res<PrimaryPlayerRes>,
) {
    let containing_scenes = containing_scenes.get(player_res.0);

    let Ok(mut current_choice) = player.single_mut() else {
        commands.entity(player_res.0).insert(Movement::default());
        return;
    };

    // clear current choice if we left the scene or it has updated
    let current_choice_valid = containing_scenes.contains(&current_choice.scene)
        && scenes
            .get(current_choice.scene)
            .is_ok_and(|ctx| ctx.last_update_frame == current_choice.scene_last_update);

    if !current_choice_valid {
        *current_choice = Default::default();
    }

    // find best choice: parcel first, then portables by most-recently spawned
    for (update, scene_ent) in q.iter().filter(|(_, scene_ent)| {
        scene_ent.id == SceneEntityId::PLAYER && containing_scenes.contains(&scene_ent.root)
    }) {
        // prioritise parcel scenes
        if !current_choice.scene_is_portable {
            continue;
        }

        let Ok(ctx) = scenes.get(scene_ent.root) else {
            continue;
        };

        // prioritise newer portables
        if ctx.is_portable && ctx.start_tick <= current_choice.scene_start_tick {
            continue;
        }

        *current_choice = Movement {
            scene: scene_ent.root,
            scene_last_update: ctx.last_update_frame,
            scene_start_tick: ctx.start_tick,
            scene_is_portable: ctx.is_portable,
            movement: *update,
        };
    }
}

pub fn apply_movement(
    mut player: Query<(&mut Transform, &mut AvatarDynamicState, &Movement), With<PrimaryUser>>,
    mut scenes: Query<(Entity, &RendererSceneContext, &mut SceneColliderData)>,
    time_res: Res<Time>,
) {
    let Ok((mut transform, mut dynamic_state, movement)) = player.single_mut() else {
        return;
    };

    if movement.movement.velocity == Vec3::ZERO {
        return;
    };

    let disabled = scenes
        .iter_mut()
        .flat_map(|(scene, ctx, mut collider_data)| {
            let results = collider_data.avatar_collisions(
                ctx.last_update_frame,
                transform.translation,
                -PLAYER_COLLIDER_OVERLAP,
            );
            if results.is_empty() {
                None
            } else {
                Some((scene, results))
            }
        })
        .collect::<HashMap<_, _>>();

    let mut position = transform.translation;
    let mut time = time_res.delta_secs();
    let mut velocity = movement.movement.velocity;
    let mut steps = 0;

    while steps < 6 && time > 0.0 {
        steps += 1;
        let mut step_time = time;
        let mut contact_normal = Vec3::ZERO;
        for (e, ctx, mut collider_data) in scenes.iter_mut() {
            if let Some(hit) = collider_data.cast_avatar_nearest(
                ctx.last_update_frame,
                position,
                velocity,
                step_time,
                ColliderLayer::ClPhysics as u32 | GROUND_COLLISION_MASK,
                false,
                false,
                disabled
                    .get(&e)
                    .map(|d| d.iter().collect())
                    .unwrap_or_default(),
                false,
                -PLAYER_COLLIDER_OVERLAP,
            ) {
                step_time = hit.toi * time;
                contact_normal = hit.normal;
            }
        }

        position += velocity * step_time + contact_normal * PLAYER_COLLIDER_OVERLAP;
        velocity = velocity - (velocity.dot(contact_normal) * contact_normal);
        time -= step_time;
    }

    transform.translation = position;
    transform.rotation = Quat::from_rotation_y(movement.movement.orientation / 360.0 * TAU);
    dynamic_state.velocity = velocity;
}

// (scene entity, collider id) of collider player is standing on
#[derive(Component, Default)]
pub struct GroundCollider(pub Option<(Entity, ColliderId, GlobalTransform)>);

fn record_ground_collider(
    mut player: Query<(Entity, &Transform, &Movement, &mut GroundCollider)>,
    containing_scenes: ContainingScene,
    mut scenes: Query<(&RendererSceneContext, &mut SceneColliderData)>,
) {
    let Ok((player_ent, transform, movement, mut ground)) = player.single_mut() else {
        return;
    };

    ground.0 = None;

    if movement.movement.ground_direction == Vec3::ZERO {
        return;
    }

    let mut best_height = PLAYER_GROUND_THRESHOLD;

    for scene in containing_scenes.get_area(player_ent, PLAYER_COLLIDER_RADIUS) {
        let Ok((ctx, mut collider_data)) = scenes.get_mut(scene) else {
            continue;
        };

        if let Some((height, collider_id)) =
            collider_data.get_ground(ctx.last_update_frame, transform.translation)
        {
            if height < best_height {
                if let Some(entity) = collider_data.get_collider_entity(&collider_id) {
                    best_height = height;
                    ground.0 = Some((entity, collider_id.clone(), Default::default()));
                }
            }
        }
    }
}

fn apply_ground_collider_movement(
    ground_transforms: Query<(&GlobalTransform, &PreviousColliderTransform)>,
    mut player: Query<(&mut Transform, &GroundCollider), With<PrimaryUser>>,
) {
    let Ok((mut transform, GroundCollider(Some((ground_entity, _, _))))) = player.single_mut()
    else {
        return;
    };

    let Ok((new_global_transform, PreviousColliderTransform(old_transform))) =
        ground_transforms.get(*ground_entity)
    else {
        return;
    };

    if new_global_transform != old_transform {
        // update rotation
        let rotation_change = new_global_transform.to_scale_rotation_translation().1
            * old_transform.to_scale_rotation_translation().1.inverse();
        // clamp to x/z plane to avoid twisting around
        let new_facing =
            ((rotation_change * Vec3::from(transform.forward())) * (Vec3::X + Vec3::Z)).normalize();
        transform.look_to(new_facing, Vec3::Y);

        // calculate updated translation
        let player_global_transform = GlobalTransform::from(*transform);
        let relative_position = player_global_transform.reparented_to(old_transform);
        let new_transform = new_global_transform.mul_transform(relative_position);
        let new_translation = new_transform.translation();
        transform.translation = new_translation;
    }
}

// fn apply_pseudo_ground_collider_movement(
//     ground_transforms: Query<(
//         Entity,
//         &ContainerEntity,
//         &HasCollider<CtCollider>,
//         &GlobalTransform,
//         &PreviousColliderTransform,
//     )>,
//     mut player: Query<(&mut Transform, &GroundCollider, &Movement), With<PrimaryUser>>,
//     mut scenes: Query<(&RendererSceneContext, &mut SceneColliderData)>,
// ) {
//     let Ok((mut transform, ground_collider, movement)) = player.single_mut() else {
//         return;
//     };

//     // gather changed colliders
//     let mut changed_colliders = ground_transforms
//         .iter()
//         .flat_map(
//             |(entity, scene_ent, collider, new_gt, PreviousColliderTransform(old_gt))| {
//                 // skip primary ground collider
//                 if ground_collider
//                     .0
//                     .as_ref()
//                     .is_some_and(|(ground_entity, _, _)| *ground_entity == entity)
//                 {
//                     return None;
//                 }

//                 let translation = new_gt.translation() - old_gt.translation();
//                 let ctc = translation.length();
//                 // skip too big movement
//                 if ctc > PLAYER_COLLIDER_RADIUS * 0.95 {
//                     return None;
//                 }

//                 // skip non-ground direction
//                 if translation.dot(movement.movement.ground_direction) >= 0.0 {
//                     return None;
//                 }

//                 Some((scene_ent.root, collider.0.clone(), translation))
//             },
//         )
//         .collect::<Vec<_>>();

//     changed_colliders.sort_unstable_by_key(|(scene, ..)| *scene);
//     let changed_colliders = changed_colliders
//         .into_iter()
//         .map(|(scene, collider, translation)| (scene, (collider, translation)))
//         .fold(
//             HashMap::<Entity, _>::default(),
//             |mut collect, (scene, data)| {
//                 collect.entry(scene).or_insert_with(Vec::default).push(data);
//                 collect
//             },
//         );

//     // calculate max adjustment
//     let mut max_adjustment = Vec3::ZERO;

//     for (scene, data) in changed_colliders {
//         let Ok((ctx, mut collider_data)) = scenes.get_mut(scene) else {
//             continue;
//         };

//         for (collider, translation) in data {
//             // cast backwards from player + movement to player, take (1-toi) * ctc
//             let result = collider_data.cast_avatar_nearest(
//                 ctx.last_update_frame,
//                 transform.translation + translation,
//                 -translation,
//                 1.0,
//                 ColliderLayer::ClPhysics as u32 | GROUND_COLLISION_MASK,
//                 true,
//                 false,
//                 Some(&collider),
//             );

//             if let Some(result) = result {
//                 let adjustment = translation * (1.0 - result.toi);
//                 if adjustment.length_squared() > max_adjustment.length_squared() {
//                     max_adjustment = adjustment;
//                 }
//             }
//         }
//     }

//     // apply
//     transform.translation += max_adjustment;
// }

fn resolve_collisions(
    mut player: Query<&mut Transform, With<PrimaryUser>>,
    mut scenes: Query<(&RendererSceneContext, &mut SceneColliderData)>,
) {
    let Ok(mut transform) = player.single_mut() else {
        return;
    };

    let mut constraint_min = Vec3::NEG_INFINITY;
    let mut constraint_max = Vec3::INFINITY;

    let mut prev = (Vec3::ZERO, Vec3::ZERO);
    let mut current_offset = Vec3::ZERO;
    let mut iteration = 0;
    while prev != (constraint_min, constraint_max) && iteration < 5 {
        for (ctx, mut collider_data) in scenes.iter_mut() {
            let (scene_min, scene_max) = collider_data.avatar_constraints(
                ctx.last_update_frame,
                transform.translation + current_offset,
            );

            constraint_min = constraint_min.max(scene_min + current_offset);
            constraint_max = constraint_max.min(scene_max + current_offset);
        }

        // vertical: satisfy floor over ceiling
        current_offset.y = current_offset.y.min(constraint_max.y).max(constraint_min.y);

        // x/z: average if squashed
        if constraint_min.x > constraint_max.x {
            current_offset.x = (constraint_min.x + constraint_max.x) * 0.5;
        } else {
            current_offset.x = current_offset.x.clamp(constraint_min.x, constraint_max.x);
        }

        if constraint_min.z > constraint_max.z {
            current_offset.z = (constraint_min.z + constraint_max.z) * 0.5;
        } else {
            current_offset.z = current_offset.z.clamp(constraint_min.z, constraint_max.z);
        }

        prev = (constraint_min, constraint_max);
        iteration += 1;
    }

    transform.translation += current_offset;
}
