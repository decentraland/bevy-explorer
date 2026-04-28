use core::f32;
use std::f32::consts::TAU;

use bevy::{
    diagnostic::FrameCount,
    math::DVec3,
    platform::collections::{HashMap, HashSet},
    prelude::*,
};
use common::{
    dynamics::{PLAYER_COLLIDER_OVERLAP, PLAYER_COLLIDER_RADIUS, PLAYER_GROUND_THRESHOLD},
    sets::{PostUpdateSets, SceneSets},
    structs::{
        AppConfig, AvatarDynamicState, EngineMovementControl, PrimaryPlayerRes, PrimaryUser,
        SceneDrivenAnim, SceneDrivenAnimationFeedback, SceneDrivenAnimationRequest,
    },
};
use comms::global_crdt::GlobalCrdtState;
use dcl::interface::{ComponentPosition, CrdtType};
use dcl_component::{
    proto_components::{
        common::Vector3,
        sdk::components::{
            AvatarAnimationState, ColliderLayer, MovementAnimation, PbAvatarLocomotionSettings,
            PbAvatarMovement, PbAvatarMovementInfo, PbPhysicsCombinedForce,
            PbPhysicsCombinedImpulse,
        },
    },
    SceneComponentId, SceneEntityId,
};
use ipfs::{
    ipfs_path::{IpfsPath, IpfsType},
    IpfsAssetServer,
};

use scene_runner::{
    renderer_context::RendererSceneContext,
    update_world::{
        avatar_modifier_area::InputModifier,
        mesh_collider::{
            ColliderId, PreviousColliderTransform, SceneColliderData, GROUND_COLLISION_MASK,
        },
        AddCrdtInterfaceExt,
    },
    ContainingScene, SceneEntity, SceneUpdates,
};

pub struct AvatarMovementPlugin;

impl Plugin for AvatarMovementPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbAvatarMovement, AvatarMovement>(
            SceneComponentId::AVATAR_MOVEMENT,
            ComponentPosition::EntityOnly,
        );
        app.add_crdt_lww_component::<PbAvatarLocomotionSettings, AvatarLocomotionSettings>(
            SceneComponentId::AVATAR_LOCOMOTION_SETTINGS,
            ComponentPosition::EntityOnly,
        );
        app.add_crdt_lww_component::<PbPhysicsCombinedImpulse, PhysicsCombinedImpulse>(
            SceneComponentId::PHYSICS_COMBINED_IMPULSE,
            ComponentPosition::EntityOnly,
        );
        app.add_crdt_lww_component::<PbPhysicsCombinedForce, PhysicsCombinedForce>(
            SceneComponentId::PHYSICS_COMBINED_FORCE,
            ComponentPosition::EntityOnly,
        );

        app.init_resource::<AvatarMovementInfo>();
        app.init_resource::<SceneDrivenAnimationFeedback>();

        app.add_systems(Update, broadcast_movement_info.in_set(SceneSets::Init));

        app.add_systems(
            Update,
            (
                ActivePlayerComponent::<AvatarMovement>::pick_latest_frame_only_by_priority,
                ActivePlayerComponent::<AvatarLocomotionSettings>::pick_by_priority,
                ActivePlayerComponent::<InputModifier>::pick_by_priority,
                update_priority_scene.after(
                    ActivePlayerComponent::<AvatarMovement>::pick_latest_frame_only_by_priority,
                ),
                update_scene_driven_animation.after(
                    ActivePlayerComponent::<AvatarMovement>::pick_latest_frame_only_by_priority,
                ),
            )
                .in_set(SceneSets::PostLoop),
        );

        app.add_systems(
            PostUpdate,
            (
                apply_ground_collider_movement,
                resolve_collisions,
                apply_impulses,
                apply_movement,
                record_ground_collider,
            )
                .chain()
                .in_set(PostUpdateSets::PlayerUpdate),
        );
    }
}

fn update_priority_scene(
    player: Query<&ActivePlayerComponent<AvatarMovement>, With<PrimaryUser>>,
    mut updates: ResMut<SceneUpdates>,
) {
    let scene = player
        .single()
        .ok()
        .map(|active| active.scene())
        .filter(|&e| e != Entity::PLACEHOLDER);

    match scene {
        Some(scene) => {
            updates.priority_scenes.insert("movement_controller", scene);
        }
        None => {
            updates.priority_scenes.remove("movement_controller");
        }
    }
}

// Resolves the active scene's `MovementAnimation.src` against the scene content map
// (path -> content hash) and writes a ready-to-play request onto the primary player's
// `SceneDrivenAnim` component for the avatar animation system to consume.
fn update_scene_driven_animation(
    mut commands: Commands,
    player: Query<(Entity, &ActivePlayerComponent<AvatarMovement>), With<PrimaryUser>>,
    scenes: Query<&RendererSceneContext>,
    ipfas: IpfsAssetServer,
    mut logged_failures: Local<HashSet<String>>,
) {
    let Ok((primary, active)) = player.single() else {
        return;
    };
    let request = (|| {
        let anim = active.component.animation.as_ref()?;
        let scene_ent = active.scene();
        if scene_ent == Entity::PLACEHOLDER {
            return None;
        }
        let ctx = scenes.get(scene_ent).ok()?;
        let ipfs_path = IpfsPath::new(IpfsType::new_content_file(
            ctx.hash.clone(),
            anim.src.to_lowercase(),
        ));
        let ipfs_ctx = ipfas.ipfs().context.blocking_read();
        let Some(content_hash) = ipfs_path.hash(&ipfs_ctx) else {
            if logged_failures.insert(anim.src.clone()) {
                warn!(
                    "scene-driven movement animation path not found in scene content map: {}",
                    anim.src
                );
            }
            return None;
        };

        // Resolve each scene-relative audio path to a content hash against the same
        // scene content map. Drop (and warn once per src) any path that doesn't resolve.
        let sounds = anim
            .sounds
            .iter()
            .filter_map(|sound_src| {
                let sound_path = IpfsPath::new(IpfsType::new_content_file(
                    ctx.hash.clone(),
                    sound_src.to_lowercase(),
                ));
                match sound_path.hash(&ipfs_ctx) {
                    Some(h) => Some(h),
                    None => {
                        if logged_failures.insert(sound_src.clone()) {
                            warn!(
                                "scene-driven movement sound path not found in scene content map: {sound_src}"
                            );
                        }
                        None
                    }
                }
            })
            .collect();

        // The `-false` suffix is a fixed part of the scene-emote URN format here;
        // loop behavior is carried separately in SceneDrivenAnimationRequest.r#loop.
        let urn = format!(
            "urn:decentraland:off-chain:scene-emote:{}-{}-false",
            ctx.hash, content_hash
        );
        Some(SceneDrivenAnimationRequest {
            src: anim.src.clone(),
            urn,
            scene_hash: ctx.hash.clone(),
            content_hash,
            r#loop: anim.r#loop,
            speed: anim.speed,
            idle: anim.idle,
            transition_seconds: anim.transition_seconds.unwrap_or(0.2),
            seek: anim.playback_time,
            sounds,
        })
    })();

    commands
        .entity(primary)
        .try_insert(SceneDrivenAnim { active: request });
}

#[derive(Component, Clone, Debug)]
pub struct AvatarMovement {
    pub velocity: Vec3,
    pub orientation: f32,
    pub ground_direction: Vec3,
    /// set for one frame when a walk_target ends: true = reached target, false = failed
    pub walk_success: Option<bool>,
    /// scene-driven movement animation request; if absent, engine falls back to velocity-based selection
    pub animation: Option<MovementAnimation>,
}

impl Default for AvatarMovement {
    fn default() -> Self {
        Self {
            velocity: Vec3::ZERO,
            orientation: 0.0,
            ground_direction: Vec3::NEG_Y,
            walk_success: None,
            animation: None,
        }
    }
}

#[derive(Component, Clone, Debug)]
pub struct AvatarLocomotionSettings(PbAvatarLocomotionSettings);

impl From<PbAvatarLocomotionSettings> for AvatarLocomotionSettings {
    fn from(value: PbAvatarLocomotionSettings) -> Self {
        Self(value)
    }
}

impl FromConfig for AvatarLocomotionSettings {
    fn from_config(config: &AppConfig) -> Self {
        Self(PbAvatarLocomotionSettings {
            walk_speed: Some(config.player_settings.walk_speed),
            jog_speed: Some(config.player_settings.jog_speed),
            run_speed: Some(config.player_settings.run_speed),
            jump_height: Some(config.player_settings.jump_height),
            run_jump_height: Some(config.player_settings.run_jump_height),
            hard_landing_cooldown: Some(0.0),
        })
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
            walk_success: value.walk_success,
            animation: value.animation,
        }
    }
}

// generic wrapper component that tracks the active provider of singleton player components
// e.g. AvatarMovement, by picking from the highest priority scene or falling back to default
// if none is found.
#[derive(Component)]
pub struct ActivePlayerComponent<C: Component> {
    scene: Entity,
    entity: Entity,
    scene_last_update: u32,
    scene_start_tick: u32,
    scene_is_portable: bool,
    pub component: C,
}

impl<C: Component> ActivePlayerComponent<C> {
    pub fn scene(&self) -> Entity {
        self.scene
    }
}

impl<C: Component + FromConfig> FromConfig for ActivePlayerComponent<C> {
    fn from_config(config: &AppConfig) -> Self {
        Self {
            scene: Entity::PLACEHOLDER,
            entity: Entity::PLACEHOLDER,
            scene_last_update: 0,
            scene_start_tick: 0,
            scene_is_portable: true,
            component: C::from_config(config),
        }
    }
}

pub trait FromConfig {
    fn from_config(config: &AppConfig) -> Self;
}

impl<T: Default> FromConfig for T {
    fn from_config(_: &AppConfig) -> Self {
        Self::default()
    }
}

#[derive(Component)]
pub struct PhysicsCombinedForce(pub PbPhysicsCombinedForce);

impl From<PbPhysicsCombinedForce> for PhysicsCombinedForce {
    fn from(value: PbPhysicsCombinedForce) -> Self {
        Self(value)
    }
}

#[derive(Component)]
pub struct PhysicsCombinedImpulse(pub PbPhysicsCombinedImpulse);

impl From<PbPhysicsCombinedImpulse> for PhysicsCombinedImpulse {
    fn from(value: PbPhysicsCombinedImpulse) -> Self {
        Self(value)
    }
}

#[derive(Resource, Default)]
pub struct AvatarMovementInfo(pub PbAvatarMovementInfo);

impl<C: Component + Clone + FromConfig> ActivePlayerComponent<C> {
    // pick from available of any write-time, based on priority
    #[allow(clippy::too_many_arguments)]
    fn pick_by_priority(
        mut commands: Commands,
        q: Query<(Entity, Ref<C>, &SceneEntity)>,
        mut removed_components: RemovedComponents<C>,
        scenes: Query<&RendererSceneContext>,
        containing_scenes: ContainingScene,
        mut player: Query<&mut ActivePlayerComponent<C>, With<PrimaryUser>>,
        player_res: Res<PrimaryPlayerRes>,
        config: Res<AppConfig>,
    ) {
        let containing_scenes = containing_scenes.get(player_res.0);

        let Ok(mut current_choice) = player.single_mut() else {
            commands
                .entity(player_res.0)
                .try_insert(ActivePlayerComponent::<C>::from_config(&config));
            return;
        };

        // clear current choice if we left the scene or the component was removed
        let current_choice_valid = containing_scenes.contains(&current_choice.scene)
            && !removed_components
                .read()
                .any(|e| e == current_choice.entity);

        if !current_choice_valid {
            *current_choice = FromConfig::from_config(&config);
        }

        // find best choice: parcel first, then portables by most-recently spawned
        for (entity, update, scene_ent) in q.iter().filter(|(_, _, scene_ent)| {
            scene_ent.id == SceneEntityId::PLAYER && containing_scenes.contains(&scene_ent.root)
        }) {
            // skip unchanged
            if current_choice.entity == entity && !update.is_changed() {
                continue;
            }

            let Ok(ctx) = scenes.get(scene_ent.root) else {
                continue;
            };

            // prioritise parcel scenes
            if !current_choice.scene_is_portable && ctx.is_portable {
                continue;
            }

            // prioritise newer portables
            if ctx.is_portable && ctx.start_tick < current_choice.scene_start_tick {
                continue;
            }

            *current_choice = ActivePlayerComponent {
                scene: scene_ent.root,
                entity,
                scene_last_update: ctx.last_update_frame,
                scene_start_tick: ctx.start_tick,
                scene_is_portable: ctx.is_portable,
                component: update.clone(),
            };

            debug!("{} chose {}", std::any::type_name::<C>(), ctx.title);
        }
    }

    // pick based on priority, only from scenes that updated the component
    // in the last available scene-tick response
    fn pick_latest_frame_only_by_priority(
        mut commands: Commands,
        q: Query<(Entity, &C, &SceneEntity), Changed<C>>,
        scenes: Query<&RendererSceneContext>,
        containing_scenes: ContainingScene,
        mut player: Query<&mut ActivePlayerComponent<C>, With<PrimaryUser>>,
        player_res: Res<PrimaryPlayerRes>,
        config: Res<AppConfig>,
    ) {
        let containing_scenes = containing_scenes.get(player_res.0);

        let Ok(mut current_choice) = player.single_mut() else {
            commands
                .entity(player_res.0)
                .try_insert(ActivePlayerComponent::<C>::from_config(&config));
            return;
        };

        // clear current choice if we left the scene or it has updated
        let current_choice_valid = containing_scenes.contains(&current_choice.scene)
            && scenes
                .get(current_choice.scene)
                .is_ok_and(|ctx| ctx.last_update_frame == current_choice.scene_last_update);

        if !current_choice_valid {
            *current_choice = FromConfig::from_config(&config);
        }

        // find best choice: parcel first, then portables by most-recently spawned
        for (entity, update, scene_ent) in q.iter().filter(|(_, _, scene_ent)| {
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

            *current_choice = ActivePlayerComponent {
                scene: scene_ent.root,
                entity,
                scene_last_update: ctx.last_update_frame,
                scene_start_tick: ctx.start_tick,
                scene_is_portable: ctx.is_portable,
                component: update.clone(),
            };

            debug!("{} chose {}", std::any::type_name::<C>(), ctx.title);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn apply_impulses(
    impulses: Query<(&PhysicsCombinedImpulse, &SceneEntity)>,
    forces: Query<(&PhysicsCombinedForce, &SceneEntity)>,
    player: Res<PrimaryPlayerRes>,
    containing_scenes: ContainingScene,
    mut last_impulses: Local<HashMap<Entity, u32>>,
    mut info: ResMut<AvatarMovementInfo>,
    time: Res<Time>,
    live_scenes: Query<Entity, With<RendererSceneContext>>,
) {
    let containing_scenes = containing_scenes.get(player.0);

    let live: HashSet<Entity> = live_scenes.iter().collect();
    last_impulses.retain(|k, _| live.contains(k));

    for (impulse, entity) in impulses {
        if last_impulses
            .get(&entity.root)
            .is_some_and(|prev_id| *prev_id == impulse.0.event_id)
        {
            continue;
        }

        last_impulses.insert(entity.root, impulse.0.event_id);
        if !containing_scenes.contains(&entity.root) {
            continue;
        }

        info.0.external_velocity = Some(
            info.0.external_velocity.unwrap_or_default() + impulse.0.vector.unwrap_or_default(),
        );
    }

    for (force, entity) in forces {
        if !containing_scenes.contains(&entity.root) {
            continue;
        }

        info.0.external_velocity = Some(
            info.0.external_velocity.unwrap_or_default()
                + force.0.vector.unwrap_or_default() * time.delta_secs(),
        );
    }
}

pub fn apply_movement(
    mut player: Query<
        (
            &mut Transform,
            &mut AvatarDynamicState,
            &ActivePlayerComponent<AvatarMovement>,
        ),
        With<PrimaryUser>,
    >,
    mut scenes: Query<(Entity, &mut SceneColliderData)>,
    time_res: Res<Time>,
    mut info: ResMut<AvatarMovementInfo>,
    mut jumping: Local<bool>,
    movement_control: Res<EngineMovementControl>,
) {
    let Ok((mut transform, mut dynamic_state, movement)) = player.single_mut() else {
        return;
    };

    info.0.step_time = time_res.delta_secs();

    let suppress = !movement_control.suppress_avatar_physics.is_empty();
    if !suppress {
        transform.rotation = Quat::from_rotation_y(movement.component.orientation / 360.0 * TAU);
    }

    if suppress || movement.component.velocity == Vec3::ZERO {
        dynamic_state.velocity = Vec3::ZERO;
        let ground_height =
            scenes
                .iter_mut()
                .fold(transform.translation.y, |gh, (_, mut collider_data)| {
                    gh.min(
                        collider_data
                            .get_ground(transform.translation)
                            .map(|(h, _)| h)
                            .unwrap_or(f32::INFINITY),
                    )
                });
        dynamic_state.ground_height = ground_height;
        return;
    };

    let disabled = scenes
        .iter_mut()
        .flat_map(|(scene, mut collider_data)| {
            let results = collider_data.avatar_central_collisions(transform.translation.as_dvec3());
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
    let mut velocity = movement.component.velocity.as_dvec3();
    let mut steps = 0;

    while steps < 60 && time > 1e-10 {
        steps += 1;
        let mut step_time = time;
        let mut contact_normal = DVec3::ZERO;
        if movement_control.suppress_clipping.is_empty() {
            for (e, mut collider_data) in scenes.iter_mut() {
                if let Some(hit) = collider_data.cast_avatar_nearest(
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
                    -PLAYER_COLLIDER_OVERLAP,
                ) {
                    step_time = hit.toi as f64;
                    contact_normal = hit.normal.as_dvec3();
                }
            }
        }

        position += velocity * step_time + contact_normal * PLAYER_COLLIDER_OVERLAP as f64;
        velocity = velocity - (velocity.dot(contact_normal).min(0.0) * contact_normal);
        time -= step_time;
    }

    debug!(
        "move {:.7} + {:.7} = {:.7} ({steps} iterations)",
        transform.translation, movement.component.velocity, position
    );

    info.0.requested_velocity = Some(Vector3::world_vec_from_vec3(&movement.component.velocity));
    info.0.actual_velocity = Some(Vector3::world_vec_from_vec3(
        &((position - transform.translation.as_dvec3()) / time_res.delta_secs_f64()).as_vec3(),
    ));

    let position = position.as_vec3();
    let velocity = velocity.as_vec3();

    transform.translation = position.with_y(position.y.max(0.0));

    // for now we hack in the old dynamic state values for animations
    dynamic_state.velocity = velocity;
    if movement.component.velocity.y > 10.0 {
        if !*jumping {
            dynamic_state.jump_time = time_res.elapsed_secs();
            *jumping = true;
        }
    } else {
        *jumping = false;
    }
    let ground_height =
        scenes
            .iter_mut()
            .fold(transform.translation.y, |gh, (_, mut collider_data)| {
                gh.min(
                    collider_data
                        .get_ground(transform.translation)
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
    mut player: Query<(
        Entity,
        &Transform,
        &ActivePlayerComponent<AvatarMovement>,
        &mut GroundCollider,
    )>,
    containing_scenes: ContainingScene,
    mut scenes: Query<&mut SceneColliderData>,
) {
    let Ok((player_ent, transform, movement, mut ground)) = player.single_mut() else {
        return;
    };

    ground.0 = None;

    if movement.component.ground_direction == Vec3::ZERO {
        return;
    }

    let mut best_height = PLAYER_GROUND_THRESHOLD;

    for scene in containing_scenes.get_area(player_ent, PLAYER_COLLIDER_RADIUS) {
        let Ok(mut collider_data) = scenes.get_mut(scene) else {
            continue;
        };

        if let Some((height, collider_id)) = collider_data.get_ground(transform.translation) {
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
    // mut info: ResMut<AvatarMovementInfo>,
    // time: Res<Time>,
    movement_control: Res<EngineMovementControl>,
) {
    if !movement_control.suppress_avatar_physics.is_empty() {
        return;
    }

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
            // don't add ground collider movement to external_velocity, else we bounce/slide off everything
            transform.translation = new_translation;
        } else {
            debug!("skipped");
        }
    }
}

fn resolve_collisions(
    mut player: Query<&mut Transform, With<PrimaryUser>>,
    mut scenes: Query<&mut SceneColliderData>,
    mut info: ResMut<AvatarMovementInfo>,
    time: Res<Time>,
    movement_control: Res<EngineMovementControl>,
) {
    if !movement_control.suppress_clipping.is_empty()
        || !movement_control.suppress_avatar_physics.is_empty()
    {
        return;
    }

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

        for mut collider_data in scenes.iter_mut() {
            // Note: collisions that intersect the avatar central segment are automatically excluded here
            let (scene_min, scene_max) =
                collider_data.avatar_constraints(transform.translation.as_dvec3() + current_offset);

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

#[allow(clippy::type_complexity)]
fn broadcast_movement_info(
    mut info: ResMut<AvatarMovementInfo>,
    active_components: Query<
        (
            Option<&ActivePlayerComponent<AvatarLocomotionSettings>>,
            Option<&ActivePlayerComponent<InputModifier>>,
        ),
        With<PrimaryUser>,
    >,
    feedback: Res<SceneDrivenAnimationFeedback>,
    mut global_crdt: ResMut<GlobalCrdtState>,
    time: Res<Time>,
) {
    let (maybe_locomotion, maybe_modifier) = active_components.single().unwrap_or_default();

    info.0.active_avatar_locomotion_settings = maybe_locomotion.map(|l| l.component.0.clone());
    info.0.active_input_modifier = maybe_modifier.and_then(|l| l.component.0.clone());
    info.0.active_animation_state = feedback.state.as_ref().map(|s| AvatarAnimationState {
        src: s.src.clone(),
        r#loop: s.r#loop,
        speed: s.speed,
        idle: s.idle,
        playback_time: s.playback_time,
        duration: s.duration,
        loop_count: s.loop_count,
    });

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
        active_avatar_locomotion_settings: None,
        active_input_modifier: None,
        walk_target: None,
        walk_threshold: None,
        active_animation_state: None,
    }
}
