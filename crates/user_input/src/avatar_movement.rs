use core::f32;
use std::f32::consts::TAU;

use bevy::{diagnostic::FrameCount, math::DVec3, platform::collections::HashMap, prelude::*};
use common::{
    dynamics::{PLAYER_COLLIDER_OVERLAP, PLAYER_COLLIDER_RADIUS, PLAYER_GROUND_THRESHOLD},
    sets::SceneSets,
    structs::{AvatarDynamicState, PrimaryPlayerRes, PrimaryUser},
};
use comms::global_crdt::GlobalCrdtState;
use dcl::interface::{ComponentPosition, CrdtType};
use dcl_component::{
    proto_components::{
        common::Vector3,
        sdk::components::{ColliderLayer, PbAvatarMovement, PbAvatarMovementInfo},
    },
    SceneComponentId, SceneEntityId,
};

use scene_runner::{
    renderer_context::RendererSceneContext,
    update_world::{
        mesh_collider::{
            ColliderId, PreviousColliderTransform, SceneColliderData, GROUND_COLLISION_MASK,
        },
        transform_and_parent::PostUpdateSets,
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

        app.init_resource::<AvatarMovementInfo>();

        app.add_systems(Update, broadcast_movement_info.in_set(SceneSets::Init));

        app.add_systems(
            PostUpdate,
            (
                apply_ground_collider_movement,
                resolve_collisions,
                pick_movement,
                apply_movement,
                record_ground_collider,
            )
                .chain()
                .in_set(PostUpdateSets::PlayerUpdate),
        );
    }
}

#[derive(Component, Clone, Copy, Debug)]
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

#[derive(Resource, Default)]
pub struct AvatarMovementInfo(pub PbAvatarMovementInfo);

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
    mut info: ResMut<AvatarMovementInfo>,
    mut jumping: Local<bool>,
) {
    let Ok((mut transform, mut dynamic_state, movement)) = player.single_mut() else {
        return;
    };

    info.0.step_time = time_res.delta_secs();

    if movement.movement.velocity == Vec3::ZERO {
        dynamic_state.velocity = Vec3::ZERO;
        let ground_height =
            scenes
                .iter_mut()
                .fold(f32::INFINITY, |gh, (_, ctx, mut collider_data)| {
                    gh.min(
                        collider_data
                            .get_ground(ctx.last_update_frame, transform.translation)
                            .map(|(h, _)| h)
                            .unwrap_or(f32::INFINITY),
                    )
                });
        dynamic_state.ground_height = ground_height;
        return;
    };

    let disabled = scenes
        .iter_mut()
        .flat_map(|(scene, ctx, mut collider_data)| {
            let results = collider_data
                .avatar_central_collisions(ctx.last_update_frame, transform.translation.as_dvec3());
            if results.is_empty() {
                None
            } else {
                Some((scene, results))
            }
        })
        .collect::<HashMap<_, _>>();

    if !disabled.is_empty() {
        warn!("move disabling {} colliders", disabled.len());
    }

    let mut position = transform.translation.as_dvec3();
    let mut time = time_res.delta_secs_f64();
    let mut velocity = movement.movement.velocity.as_dvec3();
    let mut steps = 0;

    while steps < 60 && time > 1e-10 {
        steps += 1;
        let mut step_time = time;
        let mut contact_normal = DVec3::ZERO;
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
                step_time = hit.toi as f64;
                contact_normal = hit.normal.as_dvec3();
            }
        }

        position += velocity * step_time + contact_normal * PLAYER_COLLIDER_OVERLAP as f64;
        velocity = velocity - (velocity.dot(contact_normal).min(0.0) * contact_normal);
        time -= step_time;
    }

    debug!(
        "move {:.7} + {:.7} = {:.7} ({steps} iterations)",
        transform.translation, movement.movement.velocity, position
    );

    info.0.requested_velocity = Some(Vector3::world_vec_from_vec3(&movement.movement.velocity));
    info.0.actual_velocity = Some(Vector3::world_vec_from_vec3(
        &((position - transform.translation.as_dvec3()) / time_res.delta_secs_f64()).as_vec3(),
    ));

    let position = position.as_vec3();
    let velocity = velocity.as_vec3();

    transform.translation = position;
    transform.rotation = Quat::from_rotation_y(movement.movement.orientation / 360.0 * TAU);

    // for now we hack in the old dynamic state values for animations
    dynamic_state.velocity = velocity;
    if movement.movement.velocity.y > 10.0 {
        if !*jumping {
            dynamic_state.jump_time = time_res.elapsed_secs();
            *jumping = true;
        }
    } else {
        *jumping = false;
    }
    let ground_height = scenes
        .iter_mut()
        .fold(f32::INFINITY, |gh, (_, ctx, mut collider_data)| {
            gh.min(
                collider_data
                    .get_ground(ctx.last_update_frame, transform.translation)
                    .map(|(h, _)| h)
                    .unwrap_or(f32::INFINITY),
            )
        });
    dynamic_state.ground_height = ground_height;
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
    frame: Res<FrameCount>,
    mut info: ResMut<AvatarMovementInfo>,
    time: Res<Time>,
) {
    let Ok((mut transform, GroundCollider(Some((ground_entity, _, _))))) = player.single_mut()
    else {
        return;
    };

    let Ok((
        new_global_transform,
        PreviousColliderTransform {
            prev_transform,
            updated,
        },
    )) = ground_transforms.get(*ground_entity)
    else {
        return;
    };

    if *updated == frame.0 {
        // update rotation
        let rotation_change = new_global_transform.to_scale_rotation_translation().1
            * prev_transform.to_scale_rotation_translation().1.inverse();
        // clamp to x/z plane to avoid twisting around
        let new_facing =
            ((rotation_change * Vec3::from(transform.forward())) * (Vec3::X + Vec3::Z)).normalize();
        transform.look_to(new_facing, Vec3::Y);

        // calculate updated translation
        let player_global_transform = GlobalTransform::from(*transform);
        let relative_position = player_global_transform.reparented_to(prev_transform);
        let new_transform = new_global_transform.mul_transform(relative_position);
        let new_translation = new_transform.translation();

        debug!(
            "ground collider {} + ? = {}",
            transform.translation, new_translation
        );

        if (new_translation - transform.translation).length() < 5.0 {
            let add_external_velocity =
                (new_translation - transform.translation) / time.delta_secs();
            let existing_external_velocity = info
                .0
                .external_velocity
                .as_ref()
                .map(Vector3::world_vec_to_vec3)
                .unwrap_or_default();
            info.0.external_velocity = Some(Vector3::world_vec_from_vec3(
                &(existing_external_velocity + add_external_velocity),
            ));

            transform.translation = new_translation;
        } else {
            debug!("skipped");
        }
    }
}

fn resolve_collisions(
    mut player: Query<&mut Transform, With<PrimaryUser>>,
    mut scenes: Query<(&RendererSceneContext, &mut SceneColliderData)>,
    mut info: ResMut<AvatarMovementInfo>,
    time: Res<Time>,
) {
    let Ok(mut transform) = player.single_mut() else {
        return;
    };

    let mut constraint_min = DVec3::NEG_INFINITY;
    let mut constraint_max = DVec3::INFINITY;

    let mut prev = DVec3::INFINITY;
    let mut current_offset = DVec3::ZERO;
    let mut iteration = 0;
    while (prev - current_offset).length() > PLAYER_COLLIDER_OVERLAP as f64 * 0.01 && iteration < 60
    {
        prev = current_offset;

        for (ctx, mut collider_data) in scenes.iter_mut() {
            // Note: collisions that intersect the avatar central segment are automatically excluded here
            let (scene_min, scene_max) = collider_data.avatar_constraints(
                ctx.last_update_frame,
                transform.translation.as_dvec3() + current_offset,
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

        iteration += 1;
    }

    if (constraint_min, constraint_max) != (DVec3::NEG_INFINITY, DVec3::INFINITY) {
        debug!(
            "constraining {:.7} to ({:.7}, {:.7}) -> {:.7} ({iteration} iterations)",
            transform.translation,
            constraint_min,
            constraint_max,
            transform.translation + current_offset.as_vec3()
        );
    }

    let current_offset = current_offset.as_vec3();

    if current_offset != Vec3::ZERO {
        let add_external_velocity = current_offset / time.delta_secs();
        let existing_external_velocity = info
            .0
            .external_velocity
            .as_ref()
            .map(Vector3::world_vec_to_vec3)
            .unwrap_or_default();
        info.0.external_velocity = Some(Vector3::world_vec_from_vec3(
            &(existing_external_velocity + add_external_velocity),
        ));

        transform.translation += current_offset;
    }
}

fn broadcast_movement_info(
    mut info: ResMut<AvatarMovementInfo>,
    mut global_crdt: ResMut<GlobalCrdtState>,
    time: Res<Time>,
) {
    debug!("broadcast {:?}", info.0);

    global_crdt.update_crdt(
        SceneComponentId::AVATAR_MOVEMENT_INFO,
        CrdtType::LWW_ANY,
        SceneEntityId::PLAYER,
        &info.0,
    );
    info.0 = PbAvatarMovementInfo {
        step_time: time.delta_secs(),
        previous_step_time: info.0.step_time,
        requested_velocity: None,
        actual_velocity: None,
        external_velocity: None,
    }
}
