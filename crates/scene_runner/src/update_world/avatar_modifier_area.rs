use bevy::{
    platform::collections::{HashMap, HashSet},
    prelude::*,
};

use common::{
    dynamics::{PLAYER_COLLIDER_HEIGHT, PLAYER_COLLIDER_RADIUS},
    sets::SceneSets,
    structs::{
        ActiveAvatarArea, AvatarControl, PermissionState, PlayerModifiers, PrimaryPlayerRes,
        PrimaryUser,
    },
};
use comms::global_crdt::ForeignPlayer;
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::sdk::components::{
        pb_input_modifier, AvatarControlType, AvatarModifierType, PbAvatarModifierArea,
        PbInputModifier,
    },
    SceneComponentId, SceneEntityId,
};
use wallet::Wallet;

use crate::{
    permissions::Permission, renderer_context::RendererSceneContext, ContainingScene, SceneEntity,
};

use super::AddCrdtInterfaceExt;

pub struct AvatarModifierAreaPlugin;

#[derive(Component, Debug)]
pub struct AvatarModifierArea(pub PbAvatarModifierArea);

impl From<PbAvatarModifierArea> for AvatarModifierArea {
    fn from(value: PbAvatarModifierArea) -> Self {
        Self(value)
    }
}

#[derive(Component, Debug)]
pub struct InputModifier(pub PbInputModifier);

impl From<PbInputModifier> for InputModifier {
    fn from(value: PbInputModifier) -> Self {
        Self(value)
    }
}

impl Plugin for AvatarModifierAreaPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbAvatarModifierArea, AvatarModifierArea>(
            SceneComponentId::AVATAR_MODIFIER_AREA,
            ComponentPosition::Any,
        );

        app.add_crdt_lww_component::<PbInputModifier, InputModifier>(
            SceneComponentId::INPUT_MODIFIER,
            ComponentPosition::EntityOnly,
        );

        app.add_systems(
            Update,
            (
                update_avatar_modifier_area,
                manage_foreign_player_visibility,
            )
                .chain()
                .in_set(SceneSets::PostLoop),
        );
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn update_avatar_modifier_area(
    mut commands: Commands,
    mut players: Query<
        (
            Entity,
            &GlobalTransform,
            Option<&ForeignPlayer>,
            Option<&mut PlayerModifiers>,
        ),
        Or<(With<PrimaryUser>, With<ForeignPlayer>)>,
    >,
    containing_scene: ContainingScene,
    player_res: Res<PrimaryPlayerRes>,
    areas: Query<
        (
            Entity,
            &SceneEntity,
            Option<&AvatarModifierArea>,
            Option<&InputModifier>,
            &GlobalTransform,
        ),
        Or<(With<AvatarModifierArea>, With<InputModifier>)>,
    >,
    me: Res<Wallet>,
    mut perms: Permission<Entity>,
    mut active_hide_areas: Local<HashMap<Entity, PermissionState>>,
    contexts: Query<&RendererSceneContext>,
    input_modifiers: Query<&InputModifier>,
) {
    let scenes = containing_scene.get_area(player_res.0, PLAYER_COLLIDER_RADIUS);

    // for every player
    for (player, gt, maybe_foreign, maybe_modifiers) in players.iter_mut() {
        let Some(mut modifiers) = maybe_modifiers else {
            commands.entity(player).insert(PlayerModifiers::default());
            continue;
        };

        // reset overrides
        *modifiers = PlayerModifiers {
            areas: std::mem::take(&mut modifiers.areas),
            ..PlayerModifiers::default()
        };

        let player_position = gt.translation();
        let player_id = format!(
            "{:#x}",
            maybe_foreign
                .as_ref()
                .map(|f| f.address)
                .unwrap_or(me.address().unwrap_or_default())
        );

        // utility to check if player is within a camera area
        let player_in_area = |area: &AvatarModifierArea, transform: &GlobalTransform| -> bool {
            // check exclusions
            if area.0.exclude_ids.contains(&player_id) {
                return false;
            }

            // check bounds
            let (_, rotation, translation) = transform.to_scale_rotation_translation();
            let player_relative_position = rotation.inverse() * (player_position - translation);
            let area = area.0.area.unwrap_or_default().abs_vec_to_vec3() * 0.5
                + Vec3::new(
                    PLAYER_COLLIDER_RADIUS,
                    PLAYER_COLLIDER_HEIGHT,
                    PLAYER_COLLIDER_RADIUS,
                ) * if area.0.use_collider_range.unwrap_or(true) {
                    1.0
                } else {
                    0.0
                };
            player_relative_position.clamp(-area, area) == player_relative_position
        };

        // gather areas
        for (ent, scene_ent, maybe_area, _, transform) in areas.iter() {
            let Some(area) = maybe_area else {
                continue;
            };
            let current_index = modifiers
                .areas
                .iter()
                .enumerate()
                .find(|(_, ActiveAvatarArea { entity, .. })| ent == *entity)
                .map(|(ix, _)| ix);
            let in_area = scenes.contains(&scene_ent.root) && player_in_area(area, transform);

            if in_area == current_index.is_some() {
                continue;
            }

            match current_index {
                // remove if no longer in area
                Some(index) => {
                    modifiers.areas.remove(index);
                }
                // add at end if newly entered
                None => modifiers.areas.push(ActiveAvatarArea {
                    entity: ent,
                    allow_locomotion: PermissionState::NotRequested,
                }),
            }
        }

        // lastly add input modifier
        if maybe_foreign.is_none() {
            let input_modifier = scenes
                .iter()
                .flat_map(|scene| {
                    let Ok(ctx) = contexts.get(*scene) else {
                        return None;
                    };

                    ctx.bevy_entity(SceneEntityId::PLAYER)
                        .filter(|player_ent| input_modifiers.get(*player_ent).is_ok())
                })
                .next();

            let modifier_present = input_modifier.as_ref().is_some_and(|ent| {
                modifiers
                    .areas
                    .iter()
                    .any(|ActiveAvatarArea { entity, .. }| ent == entity)
            });

            if !modifier_present {
                modifiers
                    .areas
                    .extend(input_modifier.map(|e| ActiveAvatarArea {
                        entity: e,
                        allow_locomotion: PermissionState::NotRequested,
                    }));
            }
        }

        // remove destroyed areas
        modifiers
            .areas
            .retain(|ActiveAvatarArea { entity, .. }| areas.get(*entity).is_ok());

        // for each modifier area the player is within (starting from oldest)
        let mut areas_clone = modifiers.areas.clone();
        for active_area in areas_clone.iter_mut() {
            let (_, scene_ent, maybe_area, maybe_modifier, _) =
                areas.get(active_area.entity).unwrap();

            if maybe_area.is_some_and(|area| !area.0.modifiers.is_empty()) {
                let area = maybe_area.unwrap();
                let permit = match active_hide_areas
                    .get(&scene_ent.root)
                    .unwrap_or(&PermissionState::NotRequested)
                {
                    PermissionState::Resolved(true) => true,
                    PermissionState::NotRequested => {
                        perms.check(
                            common::structs::PermissionType::HideAvatars,
                            scene_ent.root,
                            scene_ent.root,
                            None,
                            true,
                        );
                        active_hide_areas.insert(scene_ent.root, PermissionState::Pending);
                        false
                    }
                    _ => false,
                };

                if permit {
                    // apply modifiers
                    modifiers.hide |= area
                        .0
                        .modifiers
                        .contains(&(AvatarModifierType::AmtHideAvatars as i32));
                    modifiers.hide_profile |= area
                        .0
                        .modifiers
                        .contains(&(AvatarModifierType::AmtDisablePassports as i32));
                }
            }

            if let Some(movement) = maybe_area.and_then(|area| area.0.movement_settings.as_ref()) {
                let permit = maybe_foreign.is_some()
                    || match active_area.allow_locomotion {
                        PermissionState::Resolved(true) => true,
                        PermissionState::NotRequested => {
                            perms.check(
                                common::structs::PermissionType::SetLocomotion,
                                scene_ent.root,
                                active_area.entity,
                                None,
                                false,
                            );
                            active_area.allow_locomotion = PermissionState::Pending;
                            false
                        }
                        _ => false,
                    };

                if permit {
                    if movement.control_mode.is_some() {
                        modifiers.control_type = Some(match movement.control_mode() {
                            AvatarControlType::CctNone => AvatarControl::None,
                            AvatarControlType::CctRelative => AvatarControl::Relative,
                            AvatarControlType::CctTank => AvatarControl::Tank,
                        })
                    }

                    modifiers.run_speed = movement.run_speed.or(modifiers.run_speed);
                    modifiers.friction = movement.friction.or(modifiers.friction);
                    modifiers.gravity = movement.gravity.or(modifiers.gravity);
                    modifiers.jump_height = movement.jump_height.or(modifiers.jump_height);
                    modifiers.fall_speed = movement.max_fall_speed.or(modifiers.fall_speed);
                    modifiers.turn_speed = movement.turn_speed.or(modifiers.turn_speed);
                    modifiers.walk_speed = movement.walk_speed.or(modifiers.walk_speed);
                    modifiers.block_run |= !(movement.allow_weighted_movement.unwrap_or(true));
                    modifiers.block_walk |= !(movement.allow_weighted_movement.unwrap_or(true));
                }
            }

            if let Some(pb_input_modifier::Mode::Standard(modifier)) =
                maybe_modifier.and_then(|m| m.0.mode.as_ref())
            {
                let permit = maybe_foreign.is_some()
                    || match active_area.allow_locomotion {
                        PermissionState::Resolved(true) => true,
                        PermissionState::NotRequested => {
                            perms.check(
                                common::structs::PermissionType::SetLocomotion,
                                scene_ent.root,
                                active_area.entity,
                                None,
                                false,
                            );
                            active_area.allow_locomotion = PermissionState::Pending;
                            false
                        }
                        _ => false,
                    };

                if permit {
                    modifiers.block_all |= modifier.disable_all();
                    modifiers.block_run |= modifier.disable_all() || modifier.disable_run();
                    modifiers.block_walk |= modifier.disable_all() || modifier.disable_walk();
                    modifiers.block_jump |= modifier.disable_all() || modifier.disable_jump();
                    modifiers.block_emote |= modifier.disable_all() || modifier.disable_emote();
                }
            }
        }
        if maybe_foreign.is_none() {
            let allowed_areas = perms
                .drain_success(common::structs::PermissionType::SetLocomotion)
                .collect::<HashSet<_>>();
            if !allowed_areas.is_empty() {
                for area in areas_clone.iter_mut() {
                    if allowed_areas.contains(&area.entity) {
                        area.allow_locomotion = PermissionState::Resolved(true);
                    }
                }
            }

            let denied_areas = perms
                .drain_fail(common::structs::PermissionType::SetLocomotion)
                .collect::<HashSet<_>>();
            if !denied_areas.is_empty() {
                for area in areas_clone.iter_mut() {
                    if denied_areas.contains(&area.entity) {
                        area.allow_locomotion = PermissionState::Resolved(false);
                    }
                }
            }
        }

        modifiers.areas = areas_clone;

        debug!("modifiers: {modifiers:?}");
    }

    for allowed in perms
        .drain_success(common::structs::PermissionType::HideAvatars)
        .collect::<HashSet<_>>()
    {
        active_hide_areas.insert(allowed, PermissionState::Resolved(true));
    }
    for allowed in perms
        .drain_fail(common::structs::PermissionType::HideAvatars)
        .collect::<HashSet<_>>()
    {
        active_hide_areas.insert(allowed, PermissionState::Resolved(false));
    }
    active_hide_areas.retain(|ent, _| scenes.contains(ent));
}

// primary user visiblity is more complex, and is managed in user_input::manage_player_visibility
fn manage_foreign_player_visibility(
    mut players: Query<(&mut Visibility, &PlayerModifiers), With<ForeignPlayer>>,
) {
    for (mut vis, modifiers) in players.iter_mut() {
        #[allow(clippy::collapsible_else_if)]
        if modifiers.hide {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
        } else {
            if *vis != Visibility::Inherited {
                *vis = Visibility::Inherited;
            }
        }
    }
}
